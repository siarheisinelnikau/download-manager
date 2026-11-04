const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { execFileSync } = require('node:child_process');

const version = fs.readFileSync('Cargo.toml', 'utf8').match(/^version = "(\d+\.\d+\.\d+)"$/m)?.[1];
const event = JSON.parse(fs.readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8'));
const subject = event.head_commit?.message.split(/\r?\n/, 1)[0];
if (!version || process.env.GITHUB_EVENT_NAME !== 'push' ||
    process.env.GITHUB_REF !== 'refs/heads/main' || subject !== `Release DownloadManager v${version}`) {
  throw new Error('The explicit release commit must match the version in Cargo.toml.');
}

const product = 'DownloadManager';
const prefix = `${product}-v${version}`;
const stage = '.release-stage';
const windows = path.join('.release-input', `${product}-windows-amd64`);
const ubuntu = path.join('.release-input', `${product}-ubuntu-amd64`);
const exe = `${prefix}-windows-amd64.exe`;
const deb = `${prefix}-ubuntu-amd64.deb`;
const notices = `${prefix}-notices.zip`;
const notes = 'docs/RELEASE_NOTES.md';

function sha256(data) {
  return createHash('sha256').update(data).digest('hex');
}

function verifiedPackage(directory, name, checksum) {
  const file = path.join(directory, name);
  const data = fs.readFileSync(file);
  const expected = `${sha256(data)}  ${name}`;
  if (!data.length || fs.readFileSync(path.join(directory, checksum), 'utf8').trim() !== expected) {
    throw new Error(`Package checksum mismatch: ${name}`);
  }
  return { file, data };
}

const windowsPackage = verifiedPackage(windows, exe, 'SHA256SUMS.txt');
const header = windowsPackage.data;
const pe = header.length >= 64 ? header.readUInt32LE(0x3c) : header.length;
if (header.length < 256 || header.toString('ascii', 0, 2) !== 'MZ' || pe + 6 > header.length ||
    header.readUInt32LE(pe) !== 0x4550 || header.readUInt16LE(pe + 4) !== 0x8664) {
  throw new Error('Expected a Windows amd64 executable.');
}

const ubuntuPackage = verifiedPackage(ubuntu, deb, `${deb}.sha256`);
for (const [field, expected] of Object.entries({
  Package: 'download-manager',
  Version: version,
  Architecture: 'amd64',
})) {
  const actual = execFileSync('dpkg-deb', ['-f', ubuntuPackage.file, field], { encoding: 'utf8' }).trim();
  if (actual !== expected) throw new Error(`Ubuntu package ${field} mismatch: ${actual}`);
}

fs.mkdirSync(stage);
fs.writeFileSync(path.join(stage, exe), windowsPackage.data);
fs.writeFileSync(path.join(stage, deb), ubuntuPackage.data);
execFileSync('zip', [
  '-j', path.join(stage, notices),
  path.join(windows, 'LICENSE.txt'),
  path.join(windows, 'THIRD_PARTY_NOTICES.txt'),
  notes,
]);

function asset(name) {
  const data = fs.readFileSync(path.join(stage, name));
  return { name, size: data.length, digest: `sha256:${sha256(data)}` };
}

const assets = [exe, deb, notices].map(asset);
fs.writeFileSync(path.join(stage, 'SHA256SUMS.txt'),
  assets.map(({ name, digest }) => `${digest.slice(7)}  ${name}\n`).join(''));
assets.push(asset('SHA256SUMS.txt'));
fs.writeFileSync(path.join(stage, 'manifest.json'), JSON.stringify({
  kind: 'desktop', product, version, tag: `v${version}`, notes, commit: process.env.GITHUB_SHA, assets,
}));
