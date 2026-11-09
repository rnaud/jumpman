plugins {
    alias(libs.plugins.android.application)
}

java {
    toolchain {
        languageVersion = JavaLanguageVersion.of(21)
    }
}

android {
    namespace = "com.anf.jumpman.android"
    compileSdk = 37

    defaultConfig {
        applicationId = "com.anf.jumpman.android"
        // 26 is the minimum supported by bevy_audio (cpal/AAudio).
        minSdk = 26
        targetSdk = 34
        versionCode = (System.getenv("JUMPMAN_VERSION_CODE") ?: "1").toInt()
        versionName = System.getenv("JUMPMAN_VERSION_NAME") ?: "0.1.0"
        ndk {
            // The Retroid Pocket handhelds are arm64 Snapdragons.
            abiFilters.add("arm64-v8a")
        }
    }
    buildTypes {
        getByName("release") {
            isMinifyEnabled = false
            // Sideload build: signed with the auto-generated debug keystore (no store listing, no upgrade path from other keys).
            signingConfig = signingConfigs.getByName("debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }
    // Deliberately no `assets` source set: the APK carries no game data (it is imported at runtime).
}

dependencies {
    implementation(libs.appcompat)
    implementation(libs.core)
    implementation(libs.material)
    implementation(libs.games.activity)
    implementation(libs.core.ktx)
}
