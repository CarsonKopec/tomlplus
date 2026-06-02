// Gradle build for the TOML+ Java bindings (JNA over `tomlplus_ffi`).
//
// Publishing uses the Vanniktech Maven Publish plugin, which targets the new
// Sonatype Central Portal (central.sonatype.com). All credentials and signing
// material come from environment variables — see PUBLISHING.md §7.

import com.vanniktech.maven.publish.JavaLibrary
import com.vanniktech.maven.publish.JavadocJar
import com.vanniktech.maven.publish.SonatypeHost

plugins {
    `java-library`
    id("com.vanniktech.maven.publish") version "0.30.0"
}

// Maven coordinates: io.github.carsonkopec:tomlplus-java:1.0.0
group   = "io.github.carsonkopec"
version = "1.0.0"

java {
    // Build with whatever JDK is on PATH (>=17), emit 17-compatible bytecode
    // so the resulting jar runs on any Java 17+ runtime.
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
}

repositories { mavenCentral() }

dependencies {
    api("net.java.dev.jna:jna:5.14.0")
    api("com.fasterxml.jackson.core:jackson-databind:2.17.2")

    testImplementation(platform("org.junit:junit-bom:5.10.3"))
    testImplementation("org.junit.jupiter:junit-jupiter")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.test {
    useJUnitPlatform()
    // Find tomlplus_ffi.{dll,so,dylib} on JNA's search path.
    val libDir = providers.environmentVariable("TOMLPLUS_LIB_DIR")
        .orElse(rootDir.resolve("../../target/release").canonicalFile.absolutePath)
    systemProperty("jna.library.path", libDir.get())
}

// ── Publishing (Vanniktech → Sonatype Central Portal) ───────────────────────
//
// Credentials are picked up automatically from any of:
//   * Project property `mavenCentralUsername` / `mavenCentralPassword`
//     (set via `ORG_GRADLE_PROJECT_mavenCentralUsername` env var in CI)
//   * Project property `signingInMemoryKey` / `signingInMemoryKeyPassword`
//   * Or `~/.gradle/gradle.properties` for local one-off publishes.
//
// CI sets `ORG_GRADLE_PROJECT_*` env vars from the GitHub Actions secrets;
// see `.github/workflows/release.yml`.

mavenPublishing {
    publishToMavenCentral(SonatypeHost.CENTRAL_PORTAL, automaticRelease = false)

    // Only sign when we have a key in the environment; locally without
    // SIGN_KEY set, `publishToMavenLocal` / staging tasks should still work.
    if (!System.getenv("ORG_GRADLE_PROJECT_signingInMemoryKey").isNullOrBlank()
        || project.findProperty("signingInMemoryKey") != null) {
        signAllPublications()
    }

    coordinates("io.github.carsonkopec", "tomlplus-java", project.version.toString())

    configure(JavaLibrary(javadocJar = JavadocJar.Javadoc(), sourcesJar = true))

    pom {
        name.set("tomlplus-java")
        description.set("Java bindings for TOML+ — an extended configuration format with block dictionaries, annotations, and variables. Uses JNA to bridge to the Rust core via the tomlplus-ffi C library.")
        url.set("https://github.com/CarsonKopec/tomlplus")
        inceptionYear.set("2026")
        licenses {
            license {
                name.set("MIT License")
                url.set("https://opensource.org/licenses/MIT")
                distribution.set("repo")
            }
        }
        developers {
            developer {
                id.set("CarsonKopec")
                name.set("Carson Kopec")
                email.set("kopeccarson@gmail.com")
                url.set("https://github.com/CarsonKopec")
            }
        }
        scm {
            url.set("https://github.com/CarsonKopec/tomlplus")
            connection.set("scm:git:https://github.com/CarsonKopec/tomlplus.git")
            developerConnection.set("scm:git:git@github.com:CarsonKopec/tomlplus.git")
        }
        issueManagement {
            system.set("GitHub")
            url.set("https://github.com/CarsonKopec/tomlplus/issues")
        }
    }
}
