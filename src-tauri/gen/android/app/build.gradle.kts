import java.util.Properties
import groovy.json.JsonSlurper

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

val keystoreProperties = Properties().apply {
    val propFile = rootProject.file("keystore.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

fun findRustlsPlatformVerifierAndroidMavenRepo(): String {
    try {
        val process = ProcessBuilder("cargo", "metadata", "--format-version", "1")
            .directory(projectDir.parentFile.parentFile.parentFile)
            .start()
        val output = process.inputStream.bufferedReader().readText()
        process.waitFor()
        
        val json = JsonSlurper().parseText(output) as Map<String, Any>
        val packages = json["packages"] as List<Map<String, Any>>
        val pkg = packages.find { it["name"] == "rustls-platform-verifier-android" }
        if (pkg != null) {
            val manifestPath = pkg["manifest_path"] as String
            return File(File(manifestPath).parentFile, "maven").absolutePath
        }
    } catch (e: Exception) {
        println("Warning: Could not find rustls-platform-verifier-android via cargo metadata: ${e.message}")
    }
    return ""
}

android {
    compileSdk = 35
    namespace = "xyz.eremef.awaria"

    signingConfigs {
        create("release") {
            if (keystoreProperties.isNotEmpty()) {
                storeFile = keystoreProperties.getProperty("storeFile")?.let { rootProject.file(it) }
                storePassword = keystoreProperties.getProperty("storePassword")
                keyAlias = keystoreProperties.getProperty("keyAlias")
                keyPassword = keystoreProperties.getProperty("keyPassword")
            }
        }
    }

    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "xyz.eremef.awaria"
        minSdk = 24
        targetSdk = 35
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            signingConfig = signingConfigs.getByName("release")
            isMinifyEnabled = true
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
            ndk {
                debugSymbolLevel = "FULL"
            }
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
}

rust {
    rootDirRel = "../../../"
}

repositories {
    google()
    mavenCentral()
    val localMavenRepo = findRustlsPlatformVerifierAndroidMavenRepo()
    if (localMavenRepo.isNotEmpty()) {
        maven {
            url = uri(localMavenRepo)
        }
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("androidx.webkit:webkit:1.12.1")
    implementation("androidx.appcompat:appcompat:1.6.1")
    implementation("androidx.activity:activity-ktx:1.9.3")
    implementation("com.google.android.material:material:1.13.0")
    implementation("androidx.work:work-runtime-ktx:2.9.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.7.3")
    implementation("rustls:rustls-platform-verifier:0.1.1")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")