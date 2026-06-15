plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("org.mozilla.rust-android-gradle.rust-android")
}

android {
    namespace = "com.anyplug.core"
    compileSdk = 34
    ndkVersion = "27.0.12077973"

    defaultConfig {
        minSdk = 21
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }
}

cargo {
    module = "../rust/usbip-android"
    libname = "usbip_android"
    targets = listOf("arm64", "x86_64")
    profile = "release"
}

dependencies {
    // AndroidX lifecycle (foreground service base)
    // Must be 'api' — AnyPlugService extends LifecycleService
    api("androidx.lifecycle:lifecycle-service:2.6.2")
    // Must be 'api' — Kotlin extensions exposed through public API
    api("androidx.core:core-ktx:1.12.0")
    // Must be 'api' — suspend functions and CoroutineScope in public API
    api("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.7.3")

    // mDNS fallback — exposed through future discovery API
    api("org.jmdns:jmdns:3.5.9")

    // Notification support (foreground service)
    implementation("androidx.core:core:1.12.0")
}
