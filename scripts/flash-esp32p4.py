#!/usr/bin/env python3
"""Back up the existing app and update only its validated 0x200000/8M slot."""
import hashlib
import os
from pathlib import Path
import shlex
import struct
import subprocess
import importlib.util

ROOT=Path(__file__).resolve().parents[1]
STAGE=ROOT/"target/esp32p4"


def main():
    spec=importlib.util.spec_from_file_location("plico_build",ROOT/"scripts/build-esp32p4-firmware.py")
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    env,python,_=module.environment()
    port=env.get("PLICO_P4_PORT","/dev/serial/by-id/usb-1a86_USB_Single_Serial_5CF7197112-if00")
    private=STAGE/"device-backup";private.mkdir(mode=0o700,parents=True,exist_ok=True)
    os.chmod(private,0o700)

    def tool(args):
        command=[str(python),"-m","esptool","--chip","esp32p4","-p",port,"-b","2000000"]+args
        subprocess.run(["/usr/bin/sg","dialout","-c",shlex.join(command)],env=env,check=True)

    table=private/"partitions.bin"
    tool(["read_flash","0x8000","0x1000",str(table)])
    data=table.read_bytes()
    if not any(struct.unpack_from("<HBBII",data,i)==(0x50aa,0,0,0x200000,0x800000)
               for i in range(0,len(data)-31,32)):
        raise SystemExit("Existing factory app is not 0x200000/8M; refusing to flash")
    backup=private/"pre-plico-app.bin"
    if not backup.exists():
        tool(["read_flash","0x200000","0x100000",str(backup)])
        os.chmod(backup,0o600)
    # Confirm the 1MiB backup contains the entire old image, including its hash.
    old=backup.read_bytes()
    if old[0]!=0xe9 or not old[23]:raise SystemExit("Old app header/hash is unsupported; no flash performed")
    end=24
    for _ in range(old[1]):
        if end+8>len(old):raise SystemExit("Old app exceeds backup")
        size=struct.unpack_from("<I",old,end+4)[0];end+=8+size
    digest=((end+16)//16)*16
    if digest+32>len(old) or hashlib.sha256(old[:digest]).digest()!=old[digest:digest+32]:
        raise SystemExit("Complete old app hash did not validate; no flash performed")
    image=STAGE/"idf-build/plico_p4_demo.bin"
    if not image.exists() or image.stat().st_size>0x800000:raise SystemExit("Missing/oversized app")
    tool(["write_flash","--flash_mode","dio","--flash_freq","80m","--flash_size","32MB","0x200000",str(image)])
    print("Updated app only; original bootloader, partition table and NVS preserved.")


if __name__=="__main__":main()
