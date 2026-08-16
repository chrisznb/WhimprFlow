# Releasing WhimprFlow

`scripts/release.sh` builds, signs, notarizes (when it can), zips, and
optionally installs and publishes. It picks the best signing identity it can
find: a **Developer ID Application** certificate produces a notarized build
that opens on any Mac without warnings; an **Apple Development** certificate
produces a dev build (fine locally, "damaged" warning on other Macs).

```bash
scripts/release.sh                # build + sign (+ notarize), zip
scripts/release.sh --install      # ...and install to /Applications
scripts/release.sh --publish      # ...and publish the GitHub release
```

Bump the version in `src-tauri/tauri.conf.json` before running. Optional env:
`WHIMPR_IDENTITY` (force an identity), `WHIMPR_NOTARY_PROFILE` (default
`whimpr-notary`), `WHIMPR_RELEASE_NOTES` (release body text).

## One-time setup for notarized builds

Requires an [Apple Developer Program](https://developer.apple.com/programs/)
membership (99 USD/year).

**1. Create the Developer ID certificate** (once per Mac):
Xcode → Settings → Accounts → your Apple ID → Manage Certificates →
"+" → **Developer ID Application**. Verify it shows up:

```bash
security find-identity -v -p codesigning
```

**2. Store notarization credentials** (once per Mac). Create an
app-specific password at [account.apple.com](https://account.apple.com)
(Sign-In and Security → App-Specific Passwords), then:

```bash
xcrun notarytool store-credentials whimpr-notary --apple-id YOUR_APPLE_ID --team-id YOUR_TEAM_ID
```

It prompts for the app-specific password and stores everything in the
keychain — nothing lands in this repo. Your team id is shown at
developer.apple.com → Membership.

That's it: from then on `scripts/release.sh --publish` ships notarized
builds, and the Gatekeeper note disappears from the release notes
automatically.

## What notarization changes for users

Without it, macOS quarantines the download and claims the app is "damaged"
(dev certificates are machine-bound). With Developer ID + notarization +
stapling, the app opens right after dragging it to /Applications — no
Terminal, no warnings. The in-app self-updater benefits the same way.
