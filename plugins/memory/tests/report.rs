//! Texts of 内存性能 (design memory, §5.7, §5.8) beyond those the flow's tests show.

use plugin_memory::plan::GIB;
use plugin_memory::plan::KIB;
use plugin_memory::plan::MIB;
use plugin_memory::report;

#[test]
fn sizes_read_in_the_largest_whole_unit() {
    assert_eq!(report::size(256 * MIB), "256 MiB");
    assert_eq!(report::size(GIB), "1 GiB");
    assert_eq!(report::size(3 * GIB / 2), "1536 MiB");
    assert_eq!(report::size(16 * KIB), "16 KiB");
    assert_eq!(report::size(11 * MIB + MIB / 2), "11.5 MiB");
    assert_eq!(report::size(300_000_000), "286.1 MiB");
    assert_eq!(report::size(1000), "1000 字节");
}

#[test]
fn stream_gives_its_arrays_in_millions_of_elements_and_in_mebibytes() {
    assert_eq!(
        report::stream("5.10", 32, true),
        "5.10：每个数组 3200 万个元素（244 MiB，三个合计 732 MiB），每项取 10 遍中最快的一遍；\
         多线程按节点均匀绑定"
    );
    assert_eq!(
        report::stream("5.10", 2, false),
        "5.10：每个数组 200 万个元素（15 MiB，三个合计 46 MiB），每项取 10 遍中最快的一遍"
    );
    assert!(
        report::stream("5.10", 64, false).contains("6400 万个元素（488 MiB，三个合计 1465 MiB）")
    );
    assert_eq!(
        report::stream_short(30 * MIB),
        "可用内存不足：三个数组最少要 46 MiB，可用内存的一半只有 30 MiB"
    );
}

#[test]
fn notes_count_the_levels_and_space_latin_names() {
    assert_eq!(report::missing(&["L3"]), "没有 L3 的信息，跳过这一档");
    assert_eq!(
        report::missing(&["L2", "L3"]),
        "没有 L2、L3 的信息，跳过这两档"
    );
    assert_eq!(
        report::missing(&["L1", "L2", "L3"]),
        "没有 L1、L2、L3 的信息，跳过这三档"
    );
    assert_eq!(report::failed(&["STREAM"]), "STREAM 出错");
    assert_eq!(report::failed(&["sysbench", "延迟"]), "sysbench、延迟出错");
    assert_eq!(report::failed(&["延迟", "NUMA"]), "延迟、NUMA 出错");
    assert_eq!(report::column(1), "单线程");
    assert_eq!(report::column(64), "多线程（64 线程）");
}
