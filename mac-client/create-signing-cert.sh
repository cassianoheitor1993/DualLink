#!/usr/bin/env bash
# create-signing-cert.sh
# Creates a persistent self-signed code-signing certificate "DualLink Dev"
# in the login keychain.  Run once; afterwards install.sh will use it
# so macOS TCC remembers granted permissions across rebuilds.

CERT_NAME="DualLink Dev"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

# Already exists as a valid signing identity (cert + paired private key)?
if security find-identity -v -p codesigning "$KEYCHAIN" 2>/dev/null | grep -q "\"$CERT_NAME\""; then
    echo "✅  '$CERT_NAME' is already a valid signing identity — nothing to do."
    exit 0
fi

echo "▶  Creating self-signed code-signing certificate '$CERT_NAME'..."
echo "   macOS will ask for your admin password to trust the certificate."
echo ""

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

KEY="$WORK/key.pem"
CERT="$WORK/cert.pem"

# OpenSSL config with code-signing extensions
cat > "$WORK/ext.cnf" <<'EXT'
[req]
distinguished_name = req_dn
x509_extensions    = v3_codesign
prompt             = no

[req_dn]
CN = DualLink Dev

[v3_codesign]
basicConstraints       = CA:false
subjectKeyIdentifier   = hash
authorityKeyIdentifier = keyid:always
keyUsage               = critical, digitalSignature
extendedKeyUsage       = codeSigning
EXT

# 1. Generate RSA-2048 key
openssl genrsa -out "$KEY" 2048 2>/dev/null

# 2. Generate self-signed cert (10 years)
openssl req -new -x509 -days 3650 -key "$KEY" -out "$CERT" \
  -config "$WORK/ext.cnf" 2>/dev/null

# 3. Import private key (PEM) — avoids LibreSSL PKCS12 MAC issue
security import "$KEY" \
  -k "$KEYCHAIN" \
  -T /usr/bin/codesign \
  2>/dev/null || true

# 4. Import certificate (PEM)
security import "$CERT" \
  -k "$KEYCHAIN" \
  -T /usr/bin/codesign \
  2>/dev/null || true

# 5. Trust the certificate system-wide (requires admin password)
echo "   → Trusting certificate (enter your admin password if prompted)..."
if sudo security add-trusted-cert \
     -d -r trustAsRoot \
     -k /Library/Keychains/System.keychain \
     "$CERT" 2>/dev/null; then
    echo "   ✓ Certificate trusted system-wide."
else
    # Fallback: trust in user keychain only
    security add-trusted-cert \
      -r trustAsRoot \
      -k "$KEYCHAIN" \
      "$CERT" 2>/dev/null || true
    echo "   ✓ Certificate trusted in login keychain."
fi

# 6. Allow codesign to use the key without repeated UI prompts
security set-key-partition-list \
  -S "apple-tool:,apple:,codesign:" \
  -s -k "" \
  "$KEYCHAIN" 2>/dev/null || true

# Verify
if security find-identity -v -p codesigning "$KEYCHAIN" 2>/dev/null | grep -q "\"$CERT_NAME\""; then
    echo "✅  Certificate '$CERT_NAME' created and trusted."
    echo "   install.sh will now use it — TCC permissions persist across rebuilds."
else
    echo ""
    echo "⚠️  Signing identity not found after import."
    echo "   The cert needs manual trust — follow these steps ONCE (~1 min):"
    echo ""
    echo "   Option A — Keychain Access Certificate Assistant (recommended):"
    echo "     1. Open: Applications → Utilities → Keychain Access"
    echo "     2. Menu: Keychain Access → Certificate Assistant → Create a Certificate..."
    echo "     3. Name: DualLink Dev   |   Identity Type: Self Signed Root"
    echo "        Certificate Type: Code Signing   |   check 'Let me override defaults'"
    echo "     4. Click Continue through all screens → Done"
    echo "     5. Find 'DualLink Dev', double-click → Trust → Code Signing → Always Trust"
    echo "     6. Close dialog, enter your password"
    echo ""
    echo "   Option B — Re-run this script as sudo:"
    echo "     sudo ./create-signing-cert.sh"
    echo ""
    echo "   After either option: re-run ./create-signing-cert.sh to verify."
fi
