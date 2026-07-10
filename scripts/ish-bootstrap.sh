#!/bin/sh
# Fusion — iSH/Alpine iOS bootstrap script
# Installs dependencies and runs the Fusion installer
set -eu

echo "==> Fusion for iOS (iSH Alpine Linux bootstrap)"
echo

# 1. Install prerequisites inside Alpine
echo "--> Installing dependencies (curl, git, ripgrep, ca-certificates)..."
apk update
apk add curl git ripgrep ca-certificates

echo
echo "--> Running Fusion installer..."
# 2. Call the main installer script
curl -sSL https://raw.githubusercontent.com/theaungmyatmoe/fusion/main/scripts/install.sh | sh
