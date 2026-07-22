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
    // 1. Try running cargo metadata with various cargo command paths
    val cargoPaths = listOf(
        "cargo",
        File(System.getProperty("user.home"), ".cargo/bin/cargo").absolutePath,
        File(System.getProperty("user.home"), ".cargo/bin/cargo.exe").absolutePath
    )
    for (cargo in cargoPaths) {
        try {
            val process = ProcessBuilder(cargo, "metadata", "--format-version", "1")
                .directory(projectDir.parentFile.parentFile.parentFile)
                .start()
            val output = process.inputStream.bufferedReader().readText()
            val exitCode = process.waitFor()
            if (exitCode == 0) {
                val json = JsonSlurper().parseText(output) as Map<String, Any>
                val packages = json["packages"] as List<Map<String, Any>>
                val pkg = packages.find { it["name"] == "rustls-platform-verifier-android" }
                if (pkg != null) {
                    val manifestPath = pkg["manifest_path"] as String
                    val mavenPath = File(File(manifestPath).parentFile, "maven").absolutePath
                    if (File(mavenPath).exists()) {
                        return mavenPath
                    }
                }
            }
        } catch (e: Exception) {
            // Try next cargo path
        }
    }

    // 2. Direct File System Fallback: scan ~/.cargo/registry/src/ for the cached crate
    try {
        val cargoRegistrySrc = File(System.getProperty("user.home"), ".cargo/registry/src")
        if (cargoRegistrySrc.exists()) {
            val registryDirs = cargoRegistrySrc.listFiles()
            if (registryDirs != null) {
                for (registryDir in registryDirs) {
                    if (registryDir.isDirectory) {
                        val pkgDirs = registryDir.listFiles { f -> 
                            f.isDirectory && f.name.startsWith("rustls-platform-verifier-android-") 
                        }
                        if (pkgDirs != null && pkgDirs.isNotEmpty()) {
                            // Pick the first match or sort if needed
                            val mavenPath = File(pkgDirs[0], "maven").absolutePath
                            if (File(mavenPath).exists()) {
                                return mavenPath
                            }
                        }
                    }
                }
            }
        }
    } catch (e: Exception) {
        println("Warning: File system fallback for cargo registry failed: ${e.message}")
    }

    println("Warning: Could not find rustls-platform-verifier-android Maven repo")
    return ""
}

android {
    compileSdk = 36
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
        minSdk = 26
        targetSdk = 36
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
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
            isShrinkResources = true
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
    testImplementation("org.robolectric:robolectric:4.11.1")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")