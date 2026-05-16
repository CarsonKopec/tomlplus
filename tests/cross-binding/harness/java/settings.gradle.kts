rootProject.name = "xbinding-java-harness"

// Pull in the production tomlplus-java build alongside us so the harness
// can `implementation project(":tomlplus")` instead of fetching from a
// remote Maven repo.
includeBuild("../../../../bindings/tomlplus-java")
