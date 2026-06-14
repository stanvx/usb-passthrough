plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.anyplug.tv"
    compileSdk = 34

    // Disable AAPT2 cruncher — AAPT2 8.2.0 daemon crashes with
    // "Unexpected error during link" during PNG processing on CI.
    aaptOptions {
        cruncherEnabled = false
        noCompress += listOf("webp")
    }

    defaultConfig {
        applicationId = "com.anyplug.tv"
        minSdk = 21
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        compose = true
    }

    composeOptions {
        kotlinCompilerExtensionVersion = "1.5.5"
    }
}

dependencies {
    implementation(project(":app"))

    // Compose BOM (shared with app module)
    val composeBom = platform("androidx.compose:compose-bom:2023.10.01")
    implementation(composeBom)
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.activity:activity-compose:1.8.1")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.6.2")

    implementation("androidx.leanback:leanback:1.0.0")
    implementation("androidx.tvprovider:tvprovider:1.1.0")
    implementation("org.jetbrains.kotlin:kotlin-stdlib:1.9.20")

    // Debug
    debugImplementation("androidx.compose.ui:ui-tooling")

    // Test dependencies
    testImplementation("junit:junit:4.13.2")
    testImplementation("androidx.test.ext:junit:1.1.5")
    testImplementation("androidx.test.espresso:espresso-core:3.5.1")
    testImplementation("androidx.compose.ui:ui-test-junit4")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}
