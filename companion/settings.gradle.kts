pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        exclusiveContent {
            forRepository {
                maven {
                    name = "JitPackLibsu"
                    url = uri("https://jitpack.io")
                }
            }
            filter {
                includeGroup("com.github.topjohnwu.libsu")
            }
        }
    }
}

rootProject.name = "NetHopCompanion"
include(":app")

gradle.beforeProject {
    pluginManager.withPlugin("org.jetbrains.kotlin.android") {
        throw GradleException(
            "Project $path must not apply org.jetbrains.kotlin.android; AGP 9.3.1 provides built-in Kotlin.",
        )
    }
}
