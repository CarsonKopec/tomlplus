require "minitest/autorun"
require_relative "../lib/tomlplus"

class TomlplusTest < Minitest::Test
  def test_parse_and_resolve
    doc = Tomlplus.loads <<~SRC
      [server]
      @type: int
      @min: 1
      @max: 65535
      port = 8080
    SRC
    assert_equal 8080, doc.resolve("server.port")
    assert doc.has_annotation?("server.port", "type")
  end

  def test_validate_fails
    doc = Tomlplus.loads "@min: 100\nport = 1"
    assert_raises(Tomlplus::ValidationError) { Tomlplus.validate(doc) }
  end

  def test_dumps_roundtrip
    doc  = Tomlplus.loads "[a]\nx = 1"
    text = Tomlplus.dumps(doc)
    doc2 = Tomlplus.loads(text)
    assert_equal 1, doc2.resolve("a.x")
  end

  def test_vars_and_expressions
    doc = Tomlplus.loads <<~SRC
      [vars]
      base = "https://api"
      [svc]
      url = $base + "/v1"
    SRC
    assert_equal "https://api/v1", doc.resolve("svc.url")
  end
end
