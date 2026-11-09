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
    compileSdk = 36

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
    // CI signs with the project's release key (GitHub secrets ANDROID_KEYSTORE_B64 / ANDROID_KEYSTORE_PASSWORD, key alias "jumpman"),
    // so each build installs over the previous one. Without JUMPMAN_KEYSTORE (local builds, forks) it falls back to the debug key.
    val releaseKeystore = System.getenv("JUMPMAN_KEYSTORE")
    if (releaseKeystore != null) {
        signingConfigs {
            create("release") {
                storeFile = file(releaseKeystore)
                storePassword = System.getenv("JUMPMAN_KEYSTORE_PASSWORD")
                keyAlias = "jumpman"
                keyPassword = System.getenv("JUMPMAN_KEYSTORE_PASSWORD")
            }
        }
    }
    buildTypes {
        getByName("release") {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName(if (releaseKeystore != null) "release" else "debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }
    // The APK carries no game data (it is imported at runtime). The one asset is byor-src.zip, the ROM-free inputs of the import
    // (decomp sources, music template, original assets), written to src/byorAssets by make-byor-src.py before the build and
    // audited member by member by verify-apk.py.
    sourceSets {
        getByName("main") {
            assets.srcDir("src/byorAssets")
        }
    }
}

dependencies {
    implementation(libs.appcompat)
    implementation(libs.core)
    implementation(libs.material)
    implementation(libs.games.activity)
    implementation(libs.core.ktx)
}
