#!/usr/bin/env python3
"""Build the standalone Waveshare display/touch host plus Plico static library.

Uses an isolated pinned BSP checkout and build under target/esp32p4/. Does not
flash, modify the installed IDF, or depend on any gateway/business configuration.
"""
import os
from pathlib import Path
import subprocess
import sys
import io
import tarfile
import re

ROOT=Path(__file__).resolve().parents[1]
STAGE=ROOT/"target/esp32p4"
REV="dc1b2085381c9cc3a2badf8d317673cca9079c1f"


def run(command,**kwargs):
    subprocess.run([str(x) for x in command],check=True,**kwargs)


def environment():
    env=os.environ.copy()
    tools=Path(env.get("IDF_TOOLS_PATH",str(Path.home()/".espressif")))
    idf=Path(env.get("IDF_PATH",str(tools/"esp-idf/v5.5.5")))
    pyenv=Path(env.get("IDF_PYTHON_ENV_PATH",str(tools/"python_env/idf5.5_py3.12_env")))
    cc=sorted((tools/"tools/riscv32-esp-elf").glob("*/riscv32-esp-elf/bin/riscv32-esp-elf-gcc"))[-1]
    env.update(IDF_PATH=str(idf),IDF_TOOLS_PATH=str(tools),IDF_PYTHON_ENV_PATH=str(pyenv),
        ESP_IDF_VERSION="5.5",ESP_ROM_ELF_DIR=str(tools/"tools/esp-rom-elfs/20241011"),
        PLICO_CC=str(cc),CC_riscv32imafc_esp_espidf=str(cc),
        AR_riscv32imafc_esp_espidf=str(cc.with_name("riscv32-esp-elf-ar")))
    env["PATH"]=os.pathsep.join([str(pyenv/"bin"),str(cc.parent),str(Path.home()/".cargo/bin"),env["PATH"]])
    return env,pyenv/"bin/python",cc


def main():
    env,python,cc=environment()
    STAGE.mkdir(parents=True,exist_ok=True)
    vendor=STAGE/"waveshare"
    if not vendor.exists():
        seed=env.get("PLICO_P4_VENDOR_SEED")
        bsp_path="firmware/brookesia/components/esp32_p4_wifi6_touch_lcd_4_3"
        if seed:
            # Export the pinned tree (not the seed's working changes). Partial
            # clones cannot reliably serve a full local clone; no object-store
            # dependency or source symlink survives this export.
            data=subprocess.check_output(["git","archive",REV,bsp_path],cwd=seed)
            vendor.mkdir()
            with tarfile.open(fileobj=io.BytesIO(data)) as archive:
                archive.extractall(vendor,filter="data")
            (vendor/".plico-revision").write_text(REV)
        else:
            run(["git","clone","--filter=blob:none","--no-checkout",
                "https://github.com/waveshareteam/ESP32-P4-WIFI6-Touch-LCD-4.3.git",vendor])
            run(["git","sparse-checkout","set",bsp_path],cwd=vendor)
            run(["git","checkout","--detach",REV],cwd=vendor)
    marker=vendor/".plico-revision"
    actual=marker.read_text() if marker.exists() else subprocess.check_output(["git","rev-parse","HEAD"],cwd=vendor,text=True).strip()
    if actual!=REV:
        raise SystemExit("Unexpected Waveshare BSP revision")
    project=ROOT/"apps/lvgl-esp32p4/idf"
    build=STAGE/"idf-build"
    build.mkdir(exist_ok=True)
    config=build/"sdkconfig"
    if config.exists():
        text=config.read_text()
        for line in (project/"sdkconfig.defaults").read_text().splitlines():
            if not line.startswith("CONFIG_"):continue
            key,value=line.split("=",1)
            desired=f"# {key} is not set" if value=="n" else line
            pattern=rf"^(?:{key}=.*|# {key} is not set)$"
            text=re.sub(pattern,desired,text,flags=re.M) if re.search(pattern,text,re.M) else text.rstrip()+"\n"+desired+"\n"
        config.write_text(text)
    archive=STAGE/"build/riscv32imafc-esp-espidf/release/libplico_lvgl_esp32p4.a"
    archive.parent.mkdir(parents=True,exist_ok=True)
    if not archive.exists():run([cc.with_name("riscv32-esp-elf-ar"),"rcs",archive])
    env.update(PLICO_P4_BSP=str(vendor/"firmware/brookesia/components/esp32_p4_wifi6_touch_lcd_4_3"),
        PLICO_P4_ARCHIVE=str(archive),PLICO_IDF_BUILD=str(build))
    command=[python,Path(env["IDF_PATH"])/"tools/idf.py","-C",project,"-B",build,
        "-D","IDF_TARGET=esp32p4","-D",f"SDKCONFIG={build/'sdkconfig'}"]
    run(command+["reconfigure"],env=env)
    run([sys.executable,ROOT/"scripts/build-esp32p4.py"],env=env)
    run(command+["build"],env=env)
    print("App image:",build/"plico_p4_demo.bin")
    print("Use scripts/flash-esp32p4.py; never run the generated full-flash command against an existing board.")


if __name__=="__main__":main()
