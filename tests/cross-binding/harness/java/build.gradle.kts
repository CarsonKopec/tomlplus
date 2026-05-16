plugins {
    application
}

repositories { mavenCentral() }

java {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
}

dependencies {
    // Included build above (see settings.gradle.kts).
    implementation("io.github.carsonkopec:tomlplus-java")
    implementation("com.fasterxml.jackson.core:jackson-databind:2.17.2")
}

application {
    mainClass.set("com.tomlplus.test.Harness")
}

tasks.named<JavaExec>("run") {
    // Find tomlplus_ffi.{dll,so,dylib} on JNA's search path.
    val libDir = providers.environmentVariable("TOMLPLUS_LIB_DIR")
        .orElse(rootDir.resolve("../../../../target/release").canonicalFile.absolutePath)
    systemProperty("jna.library.path", libDir.get())
    standardInput = System.`in`
}
