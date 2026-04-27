# ProGuard rules for Awaria
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# Keep rustls-platform-verifier classes
-keep,includedescriptorclasses class org.rustls.** { *; }
-keep,includedescriptorclasses class org.rustls.platformverifier.** { *; }
-keep interface org.rustls.** { *; }

# Keep standard security and SSL classes that might be used by rustls via JNI
-keep class java.security.** { *; }
-keep class javax.net.ssl.** { *; }
-keep class java.security.cert.** { *; }

# Keep WidgetUtils and its native methods for JNI
-keep class xyz.eremef.awaria.WidgetUtils { *; }
-keepclassmembers class xyz.eremef.awaria.WidgetUtils {
    native <methods>;
}

# Keep MainActivity (though usually kept by default)
-keep class xyz.eremef.awaria.MainActivity { *; }

# Preserve line numbers for debugging if needed
-keepattributes SourceFile,LineNumberTable