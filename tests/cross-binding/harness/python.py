"""Python harness — parses a .tomlp file, prints config as JSON."""
import json, sys, tomlplus

doc = tomlplus.load(sys.argv[1])
json.dump(doc.config, sys.stdout, indent=2, sort_keys=True, default=str)
sys.stdout.write("\n")
