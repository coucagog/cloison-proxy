// CLOISON Mobile — projet Gradle (Android).
// Versions épinglées ici (build CLI/CI déterministe, sans Android Studio) :
// AGP 8.5.2 + Kotlin 2.0.20 ↔ compileSdk 34 + JDK 17 (app/build.gradle.kts).
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
    plugins {
        id("com.android.application") version "8.5.2"
        id("org.jetbrains.kotlin.android") version "2.0.20"
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}
rootProject.name = "cloison-android"
include(":app")
