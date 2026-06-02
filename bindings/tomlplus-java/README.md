# tomlplus (Java)

Java bindings to `tomlplus_ffi` via [JNA](https://github.com/java-native-access/jna).

## Install

```xml
<dependency>
    <groupId>io.github.carsonkopec</groupId>
    <artifactId>tomlplus-java</artifactId>
    <version>1.0.0</version>
</dependency>
```

Or Gradle:

```kotlin
implementation("io.github.carsonkopec:tomlplus-java:1.0.0")
```

JNA needs `tomlplus_ffi.dll` / `libtomlplus_ffi.so` / `libtomlplus_ffi.dylib`
on its native search path. Either install the library system-wide or point
JNA at the build dir:

```
-Djna.library.path=C:\Users\you\tomlplus\target\release
```

## Usage

```java
import com.tomlplus.Tomlplus;

try (var doc = Tomlplus.parse("""
        [server]
        port = 8080
        """)) {
    System.out.println(doc.resolve("server.port"));   // 8080
    Tomlplus.validate(doc);
    System.out.println(Tomlplus.dumps(doc));
}
```

API: `Tomlplus.parse`, `Tomlplus.validate`, `Tomlplus.dumps`, `Tomlplus.version`.
`Document` is `AutoCloseable` — use try-with-resources. Methods: `config`,
`vars`, `meta`, `resolve`, `hasAnnotation`, `validateAll`.
