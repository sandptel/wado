#!/usr/bin/env sh
# A self-signed code-signing identity `wado-dev` in the login keychain, so a rebuilt Wado.app
# keeps its Accessibility / Screen Recording / Automation grants (macOS ties them to the
# signature). Safe to re-run: it does nothing when the identity exists.
set -eu
NAME=wado-dev
KC="$HOME/Library/Keychains/login.keychain-db"
if security find-certificate -c "$NAME" "$KC" >/dev/null 2>&1; then
  echo "mac-cert: $NAME already in the login keychain"; exit 0
fi
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
cat > "$D/cfg" <<CFG
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CFG
/usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config "$D/cfg" -keyout "$D/key.pem" -out "$D/cert.pem" 2>/dev/null
/usr/bin/openssl pkcs12 -export -inkey "$D/key.pem" -in "$D/cert.pem" -name "$NAME" -out "$D/id.p12" -passout pass:wado 2>/dev/null
security import "$D/id.p12" -k "$KC" -P wado -T /usr/bin/codesign >/dev/null
echo "mac-cert: $NAME imported"
