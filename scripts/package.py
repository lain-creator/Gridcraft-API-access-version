"""Package only the built binary and audited project resources, never user configuration."""
from pathlib import Path
import argparse, hashlib, os, plistlib, shutil, subprocess, tempfile, zipfile

parser=argparse.ArgumentParser()
parser.add_argument('--platform',choices=['macos-arm64','win64'],required=True)
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args()
root=Path(__file__).resolve().parents[1]
source=root/'gridcraft'
out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
version='0.12.1-api.20261027'
archive=out/f'GridCraft-{version}-{args.platform}.zip'
if not args.binary.is_file():raise SystemExit('The compiled binary does not exist')

with tempfile.TemporaryDirectory(prefix='gridcraft-api-package-') as temp:
    temp=Path(temp)
    if args.platform=='macos-arm64':
        bundle=temp/'GridCraft.app';binary=bundle/'Contents/MacOS/GridCraft';resource=bundle/'Contents/Resources'
        binary.parent.mkdir(parents=True);resource.mkdir()
        shutil.copyfile(args.binary,binary);os.chmod(binary,0o755)
        template=(source/'packaging/macos/Info.plist.in').read_text().replace('@SHORT_VERSION@','0.12.1').replace('@VERSION@',version).replace('@BUILD_SHA@','api-20261027')
        info=plistlib.loads(template.encode());info['GridCraftLocalPatch']='20261027.3 Third-party API and selection assistant'
        (bundle/'Contents/Info.plist').write_bytes(plistlib.dumps(info));(bundle/'Contents/PkgInfo').write_bytes(b'APPL????')
        shutil.copyfile(source/'assets/app-icon/gridcraft.icns',resource/'GridCraft.icns')
        destination=resource
    else:
        bundle=temp/f'GridCraft-{version}-Win64';bundle.mkdir();shutil.copy2(args.binary,bundle/'GridCraft.exe');destination=bundle
    for name in ['LICENSE-MIT','LICENSE-APACHE','NOTICE','ATTRIBUTION.md']:
        shutil.copy2(source/name,destination/name)
        shutil.copy2(root/'craft-libs'/name,destination/('craft-libs-'+name))
    shutil.copy2(root/'README.md',destination/'使用说明.md')
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
