plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.mozilla.rust-android-gradle.rust-android")
}

android {
    namespace = "com.anyplug"
    compileSdk = 34
    ndkVersion = "27.0.12077973"

    // Disable AAPT2 cruncher — AAPT2 8.2.0 daemon crashes with
    // "Unexpected error during link" during PNG processing on CI.
    aaptOptions {
        cruncherEnabled = false
        noCompress += listOf("webp")
    }

    defaultConfig {
        applicationId = "com.anyplug"
        minSdk = 28  // Android 9 — USB Host API mature
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"

        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    buildFeatures { compose = true }
    composeOptions { kotlinCompilerExtensionVersion = "1.5.5" }
}

cargo {
    module  = "../rust/usbip-android"
    libname = "usbip_android"
    targets = listOf("arm64", "x86_64")
    profile = "release"
}

dependencies {
    // Compose BOM
    val composeBom = platform("androidx.compose:compose-bom:2023.10.01")
    implementation(composeBom)
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.activity:activity-compose:1.8.1")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.6.2")
    implementation("androidx.lifecycle:lifecycle-service:2.6.2")

    // USB Host
    // (android.hardware.usb is in the framework, no extra dep)

    // mDNS (Android native NSD)
    // (android.net.nsd is in the framework)

    // Coroutines
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.7.3")

    // DataStore for preferences
    implementation("androidx.datastore:datastore-preferences:1.0.0")

    // JmDNS (fallback if NSD unavailable, e.g., on some TV devices)
    implementation("org.jmdns:jmdns:3.5.9")

    // Debug
    debugImplementation("androidx.compose.ui:ui-tooling")

    // Test dependencies
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.11.1")
    testImplementation("androidx.test:core:1.5.0")
}
