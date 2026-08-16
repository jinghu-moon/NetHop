import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.serialization)
}

val signingValues = listOf(
    providers.environmentVariable("NETHOP_COMPANION_KEYSTORE_PATH").orNull,
    providers.environmentVariable("NETHOP_COMPANION_KEYSTORE_PASSWORD").orNull,
    providers.environmentVariable("NETHOP_COMPANION_KEY_ALIAS").orNull,
    providers.environmentVariable("NETHOP_COMPANION_KEY_PASSWORD").orNull,
)
val releaseSigningConfigured = signingValues.all { !it.isNullOrBlank() }

android {
    namespace = "com.jinghumoon.nethop.companion"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.jinghumoon.nethop.companion"
        minSdk = 33
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        buildConfigField("int", "DAEMON_PROTOCOL_MIN", "5")
        buildConfigField("int", "DAEMON_PROTOCOL_MAX", "5")
        buildConfigField("boolean", "PUBLISHABLE", releaseSigningConfigured.toString())
    }

    buildFeatures {
        buildConfig = true
    }

    signingConfigs {
        if (releaseSigningConfigured) {
            create("release") {
                storeFile = file(requireNotNull(signingValues[0]))
                storePassword = signingValues[1]
                keyAlias = signingValues[2]
                keyPassword = signingValues[3]
            }
        }
    }

    buildTypes {
        debug {
            applicationIdSuffix = ".debug"
            versionNameSuffix = "-debug"
        }
        release {
            isDebuggable = false
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }

    kotlin {
        compilerOptions {
            jvmTarget = JvmTarget.JVM_21
            allWarningsAsErrors = true
        }
    }

    lint {
        abortOnError = true
        checkReleaseBuilds = true
        warningsAsErrors = true
        disable += setOf("AndroidGradlePluginVersion", "GradleDependency", "NewerVersionAvailable", "OldTargetApi", "UseKtx")
    }

    testOptions {
        unitTests.all {
            it.testLogging {
                events("failed", "skipped")
                exceptionFormat = org.gradle.api.tasks.testing.logging.TestExceptionFormat.FULL
            }
        }
    }

    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }
}

dependencies {
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.androidx.webkit)
    implementation(libs.libsu.core)
    implementation(libs.libsu.io)

    testImplementation(libs.junit)
    testImplementation(libs.kotlin.test.junit)
    testImplementation(libs.kotlinx.coroutines.test)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.core)
    androidTestImplementation(libs.androidx.test.espresso.core)
}

tasks.register("writeReleaseRuntimeComponents") {
    val output = layout.buildDirectory.file("reports/release-runtime-components.txt")
    outputs.file(output)
    doLast {
        val components = configurations.getByName("releaseRuntimeClasspath").incoming.resolutionResult.allComponents
            .mapNotNull { component ->
                val id = component.id as? org.gradle.api.artifacts.component.ModuleComponentIdentifier
                id?.let { "${it.group}:${it.module}:${it.version}" }
            }
            .distinct()
            .sorted()
        output.get().asFile.apply {
            parentFile.mkdirs()
            writeText(components.joinToString(separator = "\n", postfix = "\n"), Charsets.UTF_8)
        }
    }
}
