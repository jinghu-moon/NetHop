import org.gradle.util.GradleVersion

plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.serialization) apply false
}

val requiredJdk = "21"
if (JavaVersion.current().majorVersion != requiredJdk) {
    throw GradleException("NetHop Companion requires JDK $requiredJdk, found ${JavaVersion.current().majorVersion}.")
}

val requiredGradle = "9.5.0"
if (GradleVersion.current() != GradleVersion.version(requiredGradle)) {
    throw GradleException("NetHop Companion requires Gradle $requiredGradle; use the checked-in wrapper.")
}

val catalog = providers.fileContents(layout.projectDirectory.file("gradle/libs.versions.toml")).asText.get()
val forbiddenDependencies = listOf("+\"", "SNAPSHOT", "compose", "material", "room", "media3", "firebase")
check(forbiddenDependencies.none(catalog::contains)) {
    "Companion dependency baseline contains a forbidden entry"
}
