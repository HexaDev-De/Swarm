// Where Swarm lives. Builds are published on GitHub Releases; update the notes
// below once release builds are signed (Apple notarization, Authenticode).
const repoUrl = 'https://github.com/HexaDev-De/Swarm';

export const project = {
  repo: repoUrl,
  clone: `${repoUrl}.git`,
  releases: `${repoUrl}/releases/latest`,
  issues: `${repoUrl}/issues`,
  security: `${repoUrl}/security/policy`,
};

// Direct downloads: release assets keep these names, so /releases/latest/download/ always
// points at the newest build. Each has a matching .sha256 beside it.
const latest = `${repoUrl}/releases/latest/download`;
export const platformDownloads = [
  { label: 'macOS', detail: 'Apple silicon', href: `${latest}/Swarm-macOS-arm64.dmg` },
  { label: 'Windows', detail: 'x64', href: `${latest}/Swarm-Windows-x64-setup.exe` },
  { label: 'Linux', detail: 'AppImage', href: `${latest}/Swarm-Linux-x86_64.AppImage` },
] as const;

export const distribution = {
  platforms: 'macOS · Windows · Linux',
  isUnsignedPreview: true,
  requirements: 'Apple silicon · macOS 12+',
  previewNotice: 'Not notarized by Apple. macOS may block the first launch.',
  appleSupportUrl: 'https://support.apple.com/en-us/102445',
};
