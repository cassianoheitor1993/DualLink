#!/usr/bin/env bash
# create-signing-cert.sh
# Creates a persistent self-signed code-signing certificate "DualLink Dev"
# in the login keychain.  Run once; afterwards install.sh will use it
# so macOS TCC remembers granted permissions across rebuilds.

CERT_NAME="DualLink Dev"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

# Already exists?
if security find-certificate -c "$CERT_NAME" "$KEYCHAIN" &>/dev/null; then
    echo "✅  Certificate '$CERT_NAME' already exists — nothing to do."
    exit 0
fi

echo "▶  Creating self-signed code-signing certificate '$CERT_NAME'..."

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

KEY="$WORK/key.pem"
CERT="$WORK/cert.pem"
P12="$WORK/cert.p12"

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

# 3. Export PKCS#12 — macOS LibreSSL uses -password, not -passout
openssl pkcs12 -export -in "$CERT" -inkey "$KEY" \
  -name "$CERT_NAME" -password pass: -out "$P12" 2>/dev/null

# 4. Import key+cert into login keychain (no passphrase flag: -P "")
security import "$P12" \
  -k "$KEYCHAIN" \
  -T /usr/bin/codesign \
  -T /usr/bin/security \
  -P "" 2>&1 | grep -v "already exists" || true

# 5. Trust the certificate for code signing
#    -d = add to admin cert store; fall back to user trust if SIP restricts -d
security add-trusted-cert \
  -d -r trustAsRoot \
  -k "$KEYCHAIN" \
  "$CERT" 2>/dev/null || \
security add-trusted-cert \
  -r trustAsRoot \
  -k "$KEYCHAIN" \
  "$CERT" 2>/dev/null || true

# 6. Allow codesign access without keychain UI prompt
#    empty -k "" means use the keychain default password (login keychain = login password)
security set-key-partition-list \
  -S "apple-tool:,apple:,codesign:" \
  -s -k "" \
  "$KEYCHAIN" 2>/dev/null || true

# Verify
if security find-certificate -c "$CERT_NAME" "$KEYCHAIN" &>/dev/null; then
    echo "✅  Certificate '$CERT_NAME' created and trusted."
    echo "   install.sh will now use it — TCC permissions persist across rebuilds."
else
    echo ""
    echo "⚠️  Certificate was not found after import."
    echo "   Try opening Keychain Access and manually:"
    echo "   1. Find '$CERT_NAME' → Get Info → Trust"
    echo "   2. Set 'Code Signing' to 'Always Trust'"
fi
