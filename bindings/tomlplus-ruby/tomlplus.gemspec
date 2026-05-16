Gem::Specification.new do |s|
  s.name        = "tomlplus"
  s.version     = "2.0.0"
  s.summary     = "TOML+ — extended configuration format (Ruby FFI bindings)"
  s.description = "Ruby bindings to the tomlplus_ffi C library."
  s.authors     = ["Carson Kopec"]
  s.email       = ["kopeccarson@gmail.com"]
  s.license     = "MIT"
  s.homepage    = "https://github.com/CarsonKopec/tomlplus"
  s.files       = Dir["lib/**/*.rb", "README.md", "LICENSE*"]
  s.required_ruby_version = ">= 3.0"
  s.add_dependency "ffi", "~> 1.16"
  s.add_development_dependency "minitest", "~> 5"
end
