#!/bin/sh
# Read-only capture of the host files that bench-host reads, as a text stream on stdout.
#
#   ssh root@host sh -s < capture.sh > host.capture
#   docker run --rm -i debian:13-slim sh -s < capture.sh > container.capture
#   ./unpack.sh host.capture <fixture directory>
#
# Nothing is written on the host. Hostnames, addresses, serial numbers and UUIDs are never
# read; of PID 1's environment only the `container` variable is kept, and of the udev
# database only the keys listed below. Review a capture before committing it (README.md).
set -u

emit_file() {
    [ -f "$1" ] && [ -r "$1" ] || return 0
    printf '@@@ FILE %s %s\n' "${1#/}" "$(base64 -w0 < "$1" 2>/dev/null)"
}

emit_dir() {
    [ -d "$1" ] && printf '@@@ DIR %s\n' "${1#/}"
    return 0
}

emit_cmd() {
    name=$1
    shift
    command -v "$1" >/dev/null 2>&1 || return 0
    printf '@@@ CMD %s %s\n' "$name" "$("$@" 2>&1 | base64 -w0)"
}

for f in /proc/cpuinfo /proc/meminfo /proc/uptime /proc/loadavg /proc/swaps /proc/self/mountinfo \
    /proc/self/cgroup /proc/sys/kernel/osrelease /proc/sys/net/ipv4/tcp_congestion_control \
    /proc/sys/net/core/default_qdisc /etc/os-release /usr/lib/os-release /etc/lsb-release \
    /etc/redhat-release /etc/centos-release /etc/system-release /etc/debian_version \
    /etc/alpine-release /sys/devices/system/cpu/online /sys/devices/system/cpu/possible \
    /sys/devices/system/cpu/present /sys/hypervisor/type /sys/hypervisor/properties/features \
    /proc/xen/capabilities /run/systemd/container /run/host/container-manager \
    /proc/device-tree/compatible /proc/device-tree/hypervisor/compatible /.dockerenv \
    /run/.containerenv /sys/fs/cgroup/memory.max /sys/fs/cgroup/memory.current \
    /sys/fs/cgroup/memory.swap.max /sys/fs/cgroup/memory.swap.current \
    /sys/fs/cgroup/memory/memory.usage_in_bytes \
    /sys/fs/cgroup/cpu.max /sys/fs/cgroup/cpuset.cpus.effective \
    /sys/fs/cgroup/memory/memory.limit_in_bytes /sys/fs/cgroup/memory/memory.memsw.limit_in_bytes \
    /sys/fs/cgroup/cpu/cpu.cfs_quota_us /sys/fs/cgroup/cpu/cpu.cfs_period_us \
    /sys/fs/cgroup/cpuset/cpuset.effective_cpus; do
    emit_file "$f"
done
for d in /proc/xen /proc/vz /proc/bc /proc/device-tree; do
    emit_dir "$d"
done
if [ -d /proc/device-tree ]; then
    for entry in /proc/device-tree/*; do
        [ -e "$entry" ] && printf '@@@ EMPTY %s\n' "${entry#/}"
    done
fi
printf '@@@ LINK proc/self/ns/pid %s\n' "$(readlink /proc/self/ns/pid)"
for f in sys_vendor product_name product_version board_vendor board_name bios_vendor \
    bios_version chassis_type; do
    emit_file "/sys/class/dmi/id/$f"
done
for cpu in /sys/devices/system/cpu/cpu[0-9]*; do
    for f in topology/physical_package_id topology/core_id topology/die_id; do
        emit_file "$cpu/$f"
    done
done
for index in /sys/devices/system/cpu/cpu0/cache/index*; do
    for f in level type size shared_cpu_list; do
        emit_file "$index/$f"
    done
done
for f in cpuinfo_max_freq cpuinfo_min_freq scaling_max_freq scaling_driver; do
    emit_file "/sys/devices/system/cpu/cpu0/cpufreq/$f"
done
for block in /sys/class/block/*; do
    for f in dev size partition ro removable queue/rotational device/model device/vendor \
        dm/name md/level; do
        emit_file "$block/$f"
    done
    for rel in slaves holders; do
        [ -d "$block/$rel" ] || continue
        emit_dir "$block/$rel"
        for entry in "$block/$rel"/*; do
            [ -e "$entry" ] && printf '@@@ EMPTY %s\n' "${entry#/}"
        done
    done
    # Only these udev keys; never serial numbers, WWNs or UUIDs.
    dev=$(cat "$block/dev" 2>/dev/null) || continue
    if [ -r "/run/udev/data/b$dev" ]; then
        keys=$(grep -E '^E:ID_(TYPE|BUS|MODEL_ENC|FS_TYPE|FS_LABEL|ATA_ROTATION_RATE_RPM)=' "/run/udev/data/b$dev")
        printf '@@@ FILE run/udev/data/b%s %s\n' "$dev" "$(printf '%s\n' "$keys" | base64 -w0)"
    fi
done
# Only the container variable of PID 1, never the rest of its environment.
if [ -r /proc/1/environ ]; then
    value=$(tr '\0' '\n' < /proc/1/environ | grep '^container=' | head -n 1)
    printf '@@@ FILE proc/1/environ %s\n' "$(printf '%s' "${value:+$value }" | tr ' ' '\0' | base64 -w0)"
fi
emit_cmd uname uname -srm
emit_cmd systemd-detect-virt systemd-detect-virt
emit_cmd lscpu lscpu
emit_cmd lsblk lsblk -b -o NAME,KNAME,TYPE,SIZE,ROTA,TRAN,MODEL,FSTYPE,MOUNTPOINTS
emit_cmd df df -B1 --output=target,size,used,avail
emit_cmd free free -b
emit_cmd nproc nproc
