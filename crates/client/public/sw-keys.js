// wado service worker — the release keys this app trusts.
//
// The one file a release owner edits. Each entry is an Ed25519 *public* key, base64 of its 32
// raw bytes, as `node scripts/release-sign.mjs keygen` prints it. A new client version is adopted
// only when its release.sig verifies against one of these (docs/release-signing.md).
//
// Empty, nothing can be adopted: an installed app keeps the version it first installed and says
// "an unsigned update was refused" until a key is added here and a release is signed with it.
//
// Rotating: add the new key next to the old one, publish and sign that release with the OLD key;
// sign the next release with the new key; then drop the old one. A private key never goes here.
//
// `dev: true` marks a throwaway key for local testing (`release-sign.mjs dev` writes one into a
// local build). sw-verify.js honours it only on localhost, never on a deployed address.
self.WADO_RELEASE_KEYS = [
  // { key: "<base64 of the 32-byte public key>", note: "release key, made 2026-10-30" },
];
