#!/usr/bin/env python3

from time import time
import argparse
import sys, subprocess
import shutil
from pathlib import Path
import tempfile
from os.path import join, exists

arch_target_mapping = {
    "riscv32": "riscv32imac-unknown-none-elf",
    "riscv64": "riscv64imac-unknown-none-elf",
}


def rustup_install_target(target):
    installed = subprocess.check_output(["rustup", "target", "list", "--installed"])
    installed_list = installed.decode("utf-8").splitlines()
    if target not in installed_list:
        run(["rustup", "target", "add", target])


def objcopy():
    exe = (
        shutil.which("rust-objcopy")
        or shutil.which("objcopy")
        or shutil.which("llvm-objcopy")
    )
    if "rust-objcopy" in exe:
        # Check if llvm-tools-preview component is installed 
        installed = subprocess.check_output(
            ["rustup", "component", "list", "--installed"]
        )
        if "llvm-tools-preview" not in installed.decode("utf-8"):
            run(["rustup", "component", "add", "llvm-tools-preview"])

    if not exe:
        run(["cargo", "install", "cargo-binutils"])
        exe = "rust-objcopy"

    def inner(input, opts=[], output=None):
        cmd = [exe] + opts
        cmd.append(input)
        if output:
            cmd.append(output)
        run(cmd)

    return inner


def copyfile(src, dst):
    shutil.copyfile(src, dst)
    shutil.copymode(src, dst)

def copy_to_build_dir(src, target_name, build_dir):
    Path(build_dir).mkdir(parents=True, exist_ok=True)
    suffix = Path().suffix
    if src.endswith(".exe"):
        target_name += ".exe"
    target_path = join(build_dir, target_name)
    copyfile(src, target_path)
    return target_path


def cmd_build(args):
    build(args.target, args.build_dir, args.release, args.arch)

def build(target, build_dir, release, arch):
    """Build target"""

    targets = {
        "kernel.bin": build_kernel,
        "mkfs.naive": build_mkfs_naive,
        "init": build_init_proc,
        "initfs.img": build_initfs_img,
    }

    if target not in targets:
        fatal("no target named `{}`.\nall targets: [{}]".format(target, ', '.join(targets.keys())))
    target_path = join(build_dir, target)
    
    if exists(target_path):
        return target_path
    if exists(target_path+".exe"):
        return target_path+".exe"

    return targets[target](release, arch, target, build_dir)

def build_kernel(release, arch, target_name, build_dir):
    """Build kernel binary, returns binary file path."""

    cargobuild = ["cargo", "-Zunstable-options", "-Zconfig-include", "--config", "include=\"kernel_cargo_config.toml\"", "build"]
    if release:
        cargobuild += ["--release"]

    target = arch_target_mapping[arch]
    rustup_install_target(target)
    cargobuild += ["--target", target]

    run(cargobuild, verbose=True)

    kernel = "target/{}/{}/kernel".format(target, "release" if release else "debug")
    kernel_bin = kernel + ".bin"
    objcopy()(kernel, ["--strip-all", "-O", "binary"], kernel_bin)
    return copy_to_build_dir(kernel_bin, target_name, build_dir)


def build_init_proc(release, arch, target_name, build_dir):
    """Build init proc, returns init proc binary file path."""
    cargobuild = ["cargo", "build", "--package", "init_proc"]
    if release:
        cargobuild += ["--release"]

    target = arch_target_mapping[arch]
    rustup_install_target(target)
    cargobuild += ["--target", target]
    run(cargobuild, verbose=True)
    src = "target/{}/{}/init_proc".format(target, "release" if release else "debug")
    return copy_to_build_dir(src, target_name, build_dir)



def build_mkfs_naive(release, _arch, target_name, build_dir):
    """Build init proc, returns mkfs binary file path."""
    cargobuild = ["cargo", "build", "--package", "mkfs", "--bin", "mkfs-naive"]
    if release:
        cargobuild += ["--release"]
    run(cargobuild, verbose=True)
    src = "target/{}/mkfs-naive".format("release" if release else "debug")
    return copy_to_build_dir(src, target_name, build_dir)


def build_initfs_img(_release, arch, target_name, build_dir):
    mkfs_naive_bin = build("mkfs.naive", build_dir, release=True, arch=arch)
    target_path = join(build_dir, target_name)
    with tempfile.TemporaryDirectory() as tempdir:
        initpath = build("init", build_dir, release=True, arch=arch)
        init_proc_target = join(tempdir, "init")
        shutil.copyfile(initpath, init_proc_target)
        shutil.copymode(initpath, init_proc_target)
        cmd = [
            mkfs_naive_bin,
            "--output",
            target_path,
            "--init-files-path",
            join(tempdir, "*"),
        ]
        run(cmd)
    return target_path


def cmd_qemu(args):
    kernel_bin_path = build("kernel.bin", args.build_dir, args.release, args.arch)
    initfs_img_path = build("initfs.img",  args.build_dir, args.release, args.arch)
    cmd = [
        "qemu-system-" + args.arch,
        "-smp",
        str(args.smp),
        "-m",
        args.ram,
        "--machine",
        "virt",
        "-nographic",
        # "-display", 
        # "curses",
        # "-monitor",
        # "stdio",
        # "-d",
        # "int",
        # "-vga",
        # "std",
        # "-device",
        # "VGA",
        "--bios",
        "default",
        "-kernel",
        kernel_bin_path,
        "-drive",
        "file={},format=raw,id=naivefs".format(initfs_img_path),
        "-device",
        "virtio-blk-device,drive=naivefs",
    ]

    if args.gdb:
        cmd += ["-s", "-S"]

    run(cmd)

def cmd_clean(args):
    if exists(args.build_dir):
        shutil.rmtree(args.build_dir)

def main():
    parser = argparse.ArgumentParser(description="Build xrs-os")
    parser.add_argument(
        "--arch",
        choices=list(arch_target_mapping.keys()),
        default="riscv64",
        help="Specify architecture",
    )
    parser.add_argument("--release", action="store_true", default=False)
    parser.add_argument(
        "--build_dir", default="./build", help="Copy final artifacts to this directory"
    )

    subparser = parser.add_subparsers(title="subcommands")

    buildcmd = subparser.add_parser("build", help="Build the specified target")
    buildcmd.add_argument("target", default="kernel.bin", help="Specified target")
    buildcmd.set_defaults(func=cmd_build)

    qemucmd = subparser.add_parser("qemu", help="Build the kernel and run on qemu")
    qemucmd.add_argument(
        "--smp", type=int, help="Specify number of CPU cores", default=4
    )
    qemucmd.add_argument(
        "--ram", "-m", dest="ram", help="Specify size of ram", default="4G"
    )
    qemucmd.add_argument(
        "--gdb", action="store_true", help="Start gdbserver", default=False
    )
    qemucmd.set_defaults(func=cmd_qemu)

    cleancmd = subparser.add_parser("clean", help="Remove artifacts that `bootstrap.py` has generated in the past")
    cleancmd.set_defaults(func=cmd_clean)

    args = parser.parse_args()
    if "func" in args:
        args.func(args)
    else:
        parser.print_help()


def run(args, verbose=False, exception=False, **kwargs):
    """Run command"""
    if verbose:
        print("running: " + " ".join(args))
    sys.stdout.flush()
    proc = subprocess.Popen(args, **kwargs)
    code = proc.wait()
    if code != 0:
        err = "failed to run: " + " ".join(args)
        if verbose or exception:
            raise RuntimeError(err)
        fatal(err)


def fatal(msg):
    sys.exit(msg)


if __name__ == "__main__":
    main()
