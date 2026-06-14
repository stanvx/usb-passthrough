plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("org.mozilla.rust-android-gradle.rust-android")
}

android {
    namespace = "com.anyplug.common"
    compileSdk = 34
    ndkVersion = "27.0.12077973"

    androidResources {
        noCompress += listOf("webp")
    }

    defaultConfig {
        minSdk = 21
        targetSdk = 34

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
    module  = "../rust/usbip-android"
    libname = "usbip_android"
    targets = listOf("arm64", "x86_64")
    profile = "release"
}

dependencies {
    // Coroutines (shared by client, server, service)
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.7.3")

    // Lifecycle service (AnyPlugService extends LifecycleService).
    // Must be api, not implementation — consuming modules (app, tv)
    // reference AnyPlugService directly and need to resolve its supertype.
    api("androidx.lifecycle:lifecycle-service:2.6.2")
    implementation("androidx.core:core:1.12.0")

    // mDNS fallback
    implementation("org.jmdns:jmdns:3.5.9")

    // Preferences
    implementation("androidx.datastore:datastore-preferences:1.0.0")

    // Test
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.11.1")
    testImplementation("androidx.test:core:1.5.0")
}
