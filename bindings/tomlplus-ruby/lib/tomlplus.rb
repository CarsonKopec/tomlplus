require "ffi"
require "json"

module Tomlplus
  VERSION = "2.0.0"

  # Low-level FFI wrapper. End users shouldn't need this.
  module Native
    extend FFI::Library

    # `ffi_lib` honours platform-specific extensions and the loader's
    # search path. Set TOMLPLUS_LIB to a fully-qualified path for testing
    # against a build that isn't installed.
    library = ENV["TOMLPLUS_LIB"] || %w[tomlplus_ffi libtomlplus_ffi]
    ffi_lib library

    attach_function :tomlplus_parse,        [:string], :pointer
    attach_function :tomlplus_to_json,      [:pointer], :pointer
    attach_function :tomlplus_meta_json,    [:pointer], :pointer
    attach_function :tomlplus_vars_json,    [:pointer], :pointer
    attach_function :tomlplus_validate,     [:pointer], :pointer
    attach_function :tomlplus_dumps,        [:pointer], :pointer
    attach_function :tomlplus_free,         [:pointer], :void
    attach_function :tomlplus_free_string,  [:pointer], :void
    attach_function :tomlplus_last_error,   [], :string
    attach_function :tomlplus_version,      [], :string
  end

  class Error          < StandardError; end
  class ParseError     < Error;         end
  class ValidationError < Error;        end

  # A parsed TOML+ document. Holds an opaque pointer to the native handle.
  class Document
    def initialize(handle)
      @handle = handle
      ObjectSpace.define_finalizer(self, self.class._finalizer(handle))
    end

    def self._finalizer(handle)
      proc { Native.tomlplus_free(handle) unless handle.nil? || handle.null? }
    end

    # Whole config tree as a Ruby Hash.
    def config
      take_json(Native.tomlplus_to_json(@handle))
    end

    # Resolved [vars] entries.
    def vars
      take_json(Native.tomlplus_vars_json(@handle))
    end

    # Annotation metadata, keyed by dotted path.
    def meta
      take_json(Native.tomlplus_meta_json(@handle))
    end

    # Walk a dotted path; returns nil when missing.
    def resolve(path)
      path.to_s.split(".").reduce(config) do |node, part|
        break nil unless node.is_a?(Hash) && node.key?(part)
        node[part]
      end
    end

    # @return [Boolean] true iff a `@name` annotation is present at +path+.
    def has_annotation?(path, name)
      Array(meta[path]).any? { |a| a["name"] == name }
    end

    # @return [Hash{String=>String}] all `@tag: k = "v"` entries at +path+.
    def tags(path)
      Array(meta[path]).each_with_object({}) do |a, h|
        next unless a["name"] == "tag" && a["arg"].is_a?(String) && a["arg"].include?("=")
        k, v = a["arg"].split("=", 2)
        h[k.strip] = v.strip.delete('"')
      end
    end

    def required_keys
      meta.select { |_, anns| anns.any? { |a| a["name"] == "required" } }.keys
    end

    def deprecated_keys
      out = []
      meta.each do |k, anns|
        anns.each do |a|
          out << [k, a["arg"].is_a?(String) ? a["arg"] : nil] if a["name"] == "deprecated"
        end
      end
      out
    end

    private

    def take_json(ptr)
      raise Error, Native.tomlplus_last_error || "null pointer" if ptr.null?
      begin
        JSON.parse(ptr.read_string)
      ensure
        Native.tomlplus_free_string(ptr)
      end
    end
  end

  # ── Top-level API ──────────────────────────────────────────────────────────

  def self.loads(source)
    h = Native.tomlplus_parse(source.to_s)
    raise ParseError, Native.tomlplus_last_error if h.null?
    Document.new(h)
  end

  def self.load(path)
    loads(File.read(path, encoding: "UTF-8"))
  end

  def self.loads_validated(source)
    doc = loads(source)
    validate(doc)
    doc
  end

  def self.load_validated(path)
    doc = self.load(path)
    validate(doc)
    doc
  end

  def self.validate(doc)
    errors = validate_all(doc)
    err = errors.find { |e| e["severity"] == "error" }
    raise ValidationError, err["message"] if err
    nil
  end

  def self.validate_all(doc)
    ptr = Native.tomlplus_validate(doc.instance_variable_get(:@handle))
    raise Error, Native.tomlplus_last_error if ptr.null?
    begin
      JSON.parse(ptr.read_string)
    ensure
      Native.tomlplus_free_string(ptr)
    end
  end

  def self.dumps(doc)
    ptr = Native.tomlplus_dumps(doc.instance_variable_get(:@handle))
    raise Error, Native.tomlplus_last_error if ptr.null?
    begin
      ptr.read_string
    ensure
      Native.tomlplus_free_string(ptr)
    end
  end
end
