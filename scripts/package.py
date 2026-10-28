"""Package only the built binary and audited project resources, never user configuration."""
from pathlib import Path
import argparse, hashlib, os, plistlib, shutil, subprocess, tempfile, zipfile

parser=argparse.ArgumentParser()
parser.add_argument('--app',choices=['gridcraft','wordcraft'],default='gridcraft')
parser.add_argument('--platform',choices=['macos-arm64','win64'],required=True)
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--version')
parser.add_argument('--patch-label',default='20261027.6 Configurable API edition')
args=parser.parse_args()
root=Path(__file__).resolve().parents[1]
source=root/args.app
name='GridCraft' if args.app=='gridcraft' else 'WordCraft'
short_version='0.12.1' if args.app=='gridcraft' else '0.11.0'
out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
version=args.version or ('0.12.1-api.20261028.1' if args.app=='gridcraft' else '0.11.0-wordcraft.20261027.2')
if not version or any(c not in '0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-' for c in version):
    raise SystemExit('Invalid package version')
archive=out/f'{name}-{version}-{args.platform}.zip'
if not args.binary.is_file():raise SystemExit('The compiled binary does not exist')

with tempfile.TemporaryDirectory(prefix='gridcraft-api-package-') as temp:
    temp=Path(temp)
    if args.platform=='macos-arm64':
        bundle=temp/f'{name}.app';binary=bundle/f'Contents/MacOS/{name}';resource=bundle/'Contents/Resources'
        binary.parent.mkdir(parents=True);resource.mkdir()
        shutil.copyfile(args.binary,binary);os.chmod(binary,0o755)
        build_sha=subprocess.run(['git','rev-parse','HEAD'],cwd=root,check=True,capture_output=True,text=True).stdout.strip()
        template=(source/'packaging/macos/Info.plist.in').read_text().replace('@SHORT_VERSION@',short_version).replace('@VERSION@',version).replace('@BUILD_SHA@',build_sha)
        info=plistlib.loads(template.encode());info[name+'LocalPatch']=args.patch_label
        (bundle/'Contents/Info.plist').write_bytes(plistlib.dumps(info));(bundle/'Contents/PkgInfo').write_bytes(b'APPL????')
        shutil.copyfile(source/f'assets/app-icon/{args.app}.icns',resource/f'{name}.icns')
        destination=resource
    else:
        bundle=temp/f'{name}-{version}-Win64';bundle.mkdir();shutil.copy2(args.binary,bundle/f'{name}.exe');destination=bundle
    for filename in ['LICENSE-MIT','LICENSE-APACHE','NOTICE','ATTRIBUTION.md']:
        shutil.copy2(source/filename,destination/filename)
        shutil.copy2(root/'craft-libs'/filename,destination/('craft-libs-'+filename))
    shutil.copy2(root/'README.md',destination/'使用说明.md')
    if args.app=='gridcraft':
        for filename in ['LICENSE-MIT','LICENSE-APACHE']:
            shutil.copy2(source/'vendor/egui'/filename,destination/('egui-'+filename))
    else:
        for filename in ['LICENSE-MIT','LICENSE-APACHE','ATTRIBUTION.md']:
            shutil.copy2(root/'craft-i18n'/filename,destination/('craft-i18n-'+filename))
        shutil.copytree(root/'third-party/wordcraft-craft-libs-1774443',destination/'wordcraft-craft-libs-1774443')
    for path in (source/'docs').glob('local-*.md'):shutil.copy2(path,destination/path.name)
    for path in (source/'assets/fonts').glob('OFL-*.txt'):shutil.copy2(path,destination/path.name)
    if args.platform=='macos-arm64':
        # File-provider metadata inherited by a new app directory is not signable.
        subprocess.run(['xattr','-cr',str(bundle)],check=True)
        subprocess.run(['codesign','--force','--deep','--sign','-',str(bundle)],check=True)
        subprocess.run(['codesign','--verify','--deep','--strict',str(bundle)],check=True)
        subprocess.run(['ditto','-c','-k','--keepParent','--norsrc','--noextattr',str(bundle),str(archive)],check=True)
    else:
        with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED,compresslevel=6) as z:
            for path in sorted(bundle.rglob('*')):
                if path.is_file():z.write(path,str(path.relative_to(temp)))
with zipfile.ZipFile(archive) as z:
    if z.testzip() is not None:raise SystemExit('Archive CRC check failed')
checksum=hashlib.sha256(archive.read_bytes()).hexdigest()
(out/(archive.name+'.sha256')).write_text(checksum+'  '+archive.name+'\n')
print(archive)
