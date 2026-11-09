#!/bin/sh
# Read-only capture of the host files that bench-host reads, as a text stream on stdout.
#
#   ssh root@host sh -s < capture.sh > host.capture
#   docker run --rm -i debian:13-slim sh -s < capture.sh > container.capture
#   ./unpack.sh host.capture <fixture directory>
#
# Nothing is written on the host. Hostnames, addresses, serial numbers and UUIDs are never
# captured: of PID 1's environment only the `container` variable is kept, of the udev database
# only the keys listed below, and of the SMBIOS table only what `scrub_smbios` leaves. Review a
# capture before committing it (README.md).
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

emit_link() {
    [ -L "$1" ] && printf '@@@ LINK %s %s\n' "${1#/}" "$(readlink "$1")"
    return 0
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
# Device-tree boards (system information design, §9.6). Never `serial-number`, nor the boot
# arguments and seeds under `chosen`.
for f in model '#address-cells' '#size-cells' chosen/u-boot,version \
    chosen/bootloader/build-timestamp; do
    emit_file "/proc/device-tree/$f"
done
for node in /proc/device-tree/memory*; do
    emit_file "$node/device_type"
    emit_file "$node/reg"
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
for f in sys_vendor product_name product_version board_vendor board_name board_version \
    bios_vendor bios_version bios_date chassis_type; do
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
# Dedicated servers' hardware (system information design, §9). `scrub_smbios` overwrites in the
# SMBIOS table, with strings of the same length so that no structure moves, the serial numbers,
# asset tags and UUID of types 1, 2 and 17, and the whole of every type the program does not
# read. It then looks for dmidecode's serial numbers, asset tags and UUIDs in the result, and
# prints nothing if any is left.
scrub_smbios='
use strict;
use warnings;
my ($path, $dmidecode) = @ARGV;
open my $in, "<:raw", $path or exit 1;
my $t = do { local $/; <$in> };
my %keep = map { $_ => 1 } 0, 1, 2, 16, 17, 38, 127;
my %ids = (1 => [7], 2 => [7, 8], 17 => [24, 25]);
my ($p, $count) = (0, 0);
while ($p + 4 <= length $t) {
    my ($type, $len) = unpack "CC", substr($t, $p, 2);
    my $end = $len < 4 ? -1 : index($t, "\0\0", $p + $len);
    last if $end < 0;
    my (@strings, @blank);
    my $at = $p + $len;
    for my $s (split /\0/, substr($t, $at, $end - $at)) {
        push @strings, [$at, length $s];
        $at += length($s) + 1;
    }
    if ($keep{$type}) {
        @blank = map { ord substr($t, $p + $_, 1) } grep { $_ < $len } @{ $ids{$type} || [] };
        substr($t, $p + 8, 16) = "\0" x 16 if $type == 1 && $len >= 24;
    } else {
        substr($t, $p + 4, $len - 4) = "\0" x ($len - 4);
        @blank = 1 .. @strings;
    }
    for my $n (grep { $_ >= 1 && $_ <= @strings } @blank) {
        my ($o, $l) = @{ $strings[$n - 1] };
        substr($t, $o, $l) = substr("FIXTURE-" . "X" x $l, 0, $l);
    }
    $p = $end + 2;
    $count++;
}
my $tail = length($t) - $p;
substr($t, $p) = "\0" x $tail;
my @values;
if ($dmidecode) {
    open my $dmi, "-|", $dmidecode or exit 1;
    while (my $line = <$dmi>) {
        push @values, $1 if $line =~ /^\s*(?:Serial Number|Asset Tag|UUID):\s*(.*?)\s*$/;
    }
}
my @files = map("/sys/class/dmi/id/$_", qw(product_serial product_uuid board_serial
    board_asset_tag chassis_serial chassis_asset_tag));
for my $file (@files, "/proc/device-tree/serial-number") {
    open my $id, "<", $file or next;
    my $v = do { local $/; <$id> } // "";
    $v =~ s/\0.*//s;
    push @values, $v =~ s/^\s+|\s+$//gr;
}
my ($checked, $left, %seen) = (0, 0);
for my $v (grep { !$seen{$_}++ } @values) {
    next if length $v < 3 or $v =~ /^(?:none|unknown|no\ dimm|default\ string|0+|f+
        |not\ (?:specified|present|available|settable|provided)
        |to\ be\ filled\ by\ o\.e\.m\.)$/xi;
    my @forms = ($v);
    if ($v =~ /^[[:xdigit:]-]{36}$/) {
        (my $hex = $v) =~ tr/-//d;
        my $raw = pack "H*", $hex;
        push @forms, $raw, pack("V v v", unpack("N n n", $raw)) . substr($raw, 8);
    }
    $checked++;
    $left += grep { index($t, $_) >= 0 } @forms;
}
print STDERR "capture.sh: dmidecode not found; only identifiers from the kernel were checked\n"
    if !$dmidecode;
if ($left) {
    print STDERR "capture.sh: $left identifiers are still in the SMBIOS table, which is left out\n";
    exit 1;
}
print STDERR "capture.sh: SMBIOS table scrubbed: $count structures, $tail bytes after them",
    " zeroed, none of $checked identifiers from dmidecode and the kernel left\n";
binmode STDOUT;
print $t;
'
# Of pci.ids: the header, and each wanted vendor with its wanted devices and their subsystems.
pci_ids_excerpt='
BEGIN {
    n = split(want, pairs, " ")
    for (i = 1; i <= n; i++) {
        split(pairs[i], id, ":")
        vendors[id[1]] = 1
        devices[id[1] " " id[2]] = 1
    }
}
/^#/ { if (!started) print; next }
/^C / { exit }
/^[0-9a-f][0-9a-f][0-9a-f][0-9a-f] / {
    started = 1
    vendor = substr($0, 1, 4)
    device = ""
    if (vendor in vendors) print
    next
}
/^\t[0-9a-f][0-9a-f][0-9a-f][0-9a-f] / {
    device = vendor " " substr($0, 2, 4)
    if (device in devices) print
    next
}
/^\t\t/ { if (device in devices) print }
'
# dmidecode's decoding of the types the scrubbed table keeps, without the identifier lines.
dmidecode_kept() {
    dmidecode -t 0,1,2,16,17,38 | grep -v -E '^[[:space:]]*(Serial Number|Asset Tag|UUID):'
}
emit_file /sys/firmware/dmi/tables/smbios_entry_point
if [ -r /sys/firmware/dmi/tables/DMI ]; then
    if command -v perl >/dev/null 2>&1; then
        table=$(perl -e "$scrub_smbios" /sys/firmware/dmi/tables/DMI \
            "$(command -v dmidecode)" | base64 -w0)
        [ -n "$table" ] && printf '@@@ FILE sys/firmware/dmi/tables/DMI %s\n' "$table"
    else
        echo 'capture.sh: perl not found; the SMBIOS table is left out' >&2
    fi
fi
if [ -r /sys/firmware/efi/systab ]; then
    printf '@@@ FILE sys/firmware/efi/systab %s\n' \
        "$(grep -E '^SMBIOS3?=' /sys/firmware/efi/systab | base64 -w0)"
fi
for spmi in /sys/firmware/acpi/tables/SPMI*; do
    emit_file "$spmi"
done
for device in /sys/bus/acpi/devices/IPI0001:*; do
    emit_dir "$device"
    emit_file "$device/status"
done
# Never the BMC's guid.
for bmc in /sys/bus/platform/devices/ipmi_bmc.*; do
    for f in firmware_revision ipmi_version manufacturer_id; do
        emit_file "$bmc/$f"
    done
done
for mc in /sys/devices/system/edac/mc/mc[0-9]*; do
    emit_file "$mc/ce_count"
    emit_file "$mc/ue_count"
    for dimm in "$mc"/dimm[0-9]* "$mc"/rank[0-9]*; do
        emit_file "$dimm/dimm_edac_mode"
    done
    for row in "$mc"/csrow[0-9]*; do
        emit_file "$row/edac_mode"
    done
done
raid=
for device in /sys/bus/pci/devices/*; do
    class=$(cat "$device/class" 2>/dev/null) || continue
    case $class in
        0x01*) ;;
        *) continue ;;
    esac
    for f in class vendor device subsystem_vendor subsystem_device; do
        emit_file "$device/$f"
    done
    emit_link "$device/driver"
    case $class in
        0x0104*) raid="$raid $(cut -c3- "$device/vendor"):$(cut -c3- "$device/device")" ;;
    esac
done
for block in /sys/block/*; do
    emit_link "$block"
done
for ids in /usr/share/hwdata/pci.ids /usr/share/misc/pci.ids /usr/share/pci.ids; do
    [ -n "$raid" ] && [ -r "$ids" ] || continue
    printf '@@@ FILE %s %s\n' "${ids#/}" \
        "$(awk -v want="$raid" "$pci_ids_excerpt" "$ids" | base64 -w0)"
    break
done
emit_cmd uname uname -srm
emit_cmd systemd-detect-virt systemd-detect-virt
emit_cmd lscpu lscpu
emit_cmd lsblk lsblk -b -o NAME,KNAME,TYPE,SIZE,ROTA,TRAN,MODEL,FSTYPE,MOUNTPOINTS
emit_cmd df df -B1 --output=target,size,used,avail
emit_cmd free free -b
emit_cmd nproc nproc
if command -v dmidecode >/dev/null 2>&1; then
    emit_cmd dmidecode dmidecode_kept
fi
