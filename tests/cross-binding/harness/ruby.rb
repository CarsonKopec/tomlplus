# Ruby harness — parses a .tomlp file via the gem, prints JSON.
require "tomlplus"
require "json"

doc = Tomlplus.load(ARGV[0])
STDOUT.write(JSON.pretty_generate(doc.config) + "\n")
