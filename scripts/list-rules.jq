# The rules that the lists in servers/ share: an IPv4 address, an IPv6 address, a DNS name and a
# text. The scripts beside this file that check the lists (iperf3-servers.sh,
# traceroute-targets.sh and netflix-ix.sh) read it before their own rules. The rules match the
# program's readers: lists::host and lists::printable in plugins/network/src/trace/lists.rs, and
# their twins in plugins/network/src/iperf3/servers.rs. The lists' fixtures, which the scripts and
# the program both read, hold every edge of them.

# whole(pattern): the whole text matches the pattern. The end is \z, the very end of the text:
# jq's $ also matches before a final newline, which the program's readers refuse.
def whole($pattern): test("^(?:" + $pattern + ")\\z");
def ipv4_octet: "(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])";
def ipv4: whole("(" + ipv4_octet + "\\.){3}" + ipv4_octet);
# Eight groups, or fewer with one :: that stands for at least one more. An IPv4 address may stand
# for the last two groups, as in ::ffff:192.0.2.1, and only at the very end: the program's readers
# refuse one before the ::, as in 2001:db8:192.0.2.1::.
def ipv6:
    (split("::") as $halves
    | ($halves | length) as $parts
    | [$halves[] | if . == "" then [] else split(":") end] as $sides
    | ($sides | add // []) as $groups
    | ($groups | length) as $count
    | ($groups | if length > 0 and (.[-1] | ipv4) then $count + 1 else $count end) as $width
    | whole("[0-9A-Fa-f:.]+") and contains(":") and $parts <= 2
        and all($groups | to_entries[];
            (.value | whole("[0-9A-Fa-f]{1,4}"))
            or (.key == $count - 1 and (.value | ipv4)))
        and ((contains(".") | not) or (split(":") | .[-1] | ipv4))
        and (if $parts == 2 then $width <= 7 else $width == 8 end));
# Labels of 1 to 63 letters, digits and -, none at either end of a label, joined by dots: 1 to
# 253 characters in all.
def dns_name:
    length >= 1 and length <= 253
    and all(split(".")[]; whole("[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?"));
# printable(longest): a text of 1 to `longest` characters, none of them a control character.
def printable($longest):
    type == "string" and length >= 1 and length <= $longest and (test("\\p{Cc}") | not);
