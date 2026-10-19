# Security

Brevier reads pages written by strangers, so a hole in it is a hole on the reader's
computer or phone. Please report one privately: in this repository, open **Security**,
then **Report a vulnerability**. Don't open a public issue for it.

Useful to include: what a page, feed, file or repository has to contain, what happens
then, and the version (`brevier --version`; on a phone, the system's App info for
Brevier).

Only the latest release is fixed. A fix ships as a new version; a published release is
never replaced.

## How to tell a real Brevier

- **Android:** the APK, from Releases and from F-Droid alike, is signed with the key
  whose SHA-256 fingerprint is
  `30:A4:32:B8:2F:3E:F0:FC:E5:F4:68:1F:D7:F9:FE:20:5F:4B:17:A2:F4:34:19:C2:45:0C:A2:6B:E6:8D:34:80`.
- **Flatpak:** the repository at `https://brevier.bizibah.com/flatpak/repo/` and the
  bundles in Releases are signed with the OpenPGP key
  `6B4E DDB5 9942 1591 7552  C75F CE2B EBF8 1389 1B3B`
  ([public key](https://brevier.bizibah.com/flatpak/brevier.gpg)).
- **Releases:** every release lists `SHA256SUMS` for its files. Tags `v*` cannot be
  moved or deleted, and `main` cannot be rewritten.
- **Windows:** the installer is not signed yet.
