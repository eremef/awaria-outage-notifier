FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

# Avoid interactive prompts during apt installations
ENV DEBIAN_FRONTEND=noninteractive

# Install Tauri v2 system dependencies, OpenJDK 17, and unzip for Android SDK
RUN apt-get update && apt-get install -y \
    build-essential \
    curl \
    wget \
    pkg-config \
    libssl-dev \
    libgtk-3-dev \
    libwebkit2gtk-4.1-dev \
    libappindicator3-dev \
    librsvg2-dev \
    patchelf \
    libasound2-dev \
    openjdk-17-jdk \
    unzip \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

# Install extra rustup targets for Android and desktop cross-compilation
RUN rustup target add \
    x86_64-unknown-linux-gnu \
    aarch64-linux-android \
    armv7-linux-androideabi \
    i686-linux-android \
    x86_64-linux-android

# Set up environment variables
ENV CARGO_TERM_COLOR=always

# Set up Android and Java Environment Variables
ENV JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64
ENV ANDROID_HOME=/usr/local/lib/android/sdk
ENV ANDROID_SDK_ROOT=$ANDROID_HOME
ENV PATH=$PATH:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools

# Download and install Android SDK Command Line Tools
RUN mkdir -p "$ANDROID_HOME" && \
    curl -sSLo sdk.zip https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip && \
    unzip -q sdk.zip -d "$ANDROID_HOME" && \
    rm sdk.zip && \
    mkdir -p "$ANDROID_HOME/cmdline-tools/latest" && \
    mv "$ANDROID_HOME/cmdline-tools/bin" "$ANDROID_HOME/cmdline-tools/latest/" && \
    mv "$ANDROID_HOME/cmdline-tools/lib" "$ANDROID_HOME/cmdline-tools/latest/" && \
    mv "$ANDROID_HOME/cmdline-tools/source.properties" "$ANDROID_HOME/cmdline-tools/latest/"

# Accept licenses and install platform tools, platforms (33, 34), build tools (34.0.0), and NDK (26.1.10909125)
RUN yes | sdkmanager --licenses && \
    sdkmanager \
    "platform-tools" \
    "platforms;android-33" \
    "platforms;android-34" \
    "build-tools;34.0.0" \
    "ndk;26.1.10909125" && \
    chown -R vscode:vscode "$ANDROID_HOME" || true
