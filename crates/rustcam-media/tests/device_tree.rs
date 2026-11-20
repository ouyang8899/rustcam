use rustcam_media::domain::device_tree::{
    build_device_tree, DeviceTreeChannel, DeviceTreeDevice, DeviceTreeGroup, DeviceTreeNode,
    DeviceTreeRegion, NODE_CHANNEL, NODE_DEVICE, NODE_GROUP, NODE_REGION,
};

/// 后端合成的虚拟节点类型（前端按 type 识别）。
const ALL_TYPE: &str = "all";
const UNGROUPED_TYPE: &str = "ungrouped";
/// 虚拟节点的 key。
const ALL_KEY: &str = "all";
const UNGROUPED_KEY: &str = "ungrouped";

fn group(id: i64, name: &str, parent_id: Option<i64>) -> DeviceTreeGroup {
    DeviceTreeGroup {
        id,
        name: name.into(),
        parent_id,
        sort_order: 0,
    }
}

fn region(code: &str, name: &str, parent_code: Option<&str>) -> DeviceTreeRegion {
    DeviceTreeRegion {
        code: code.into(),
        name: name.into(),
        parent_code: parent_code.map(|s| s.to_string()),
    }
}

fn device(
    id: i64,
    name: &str,
    tag: &str,
    group_id: Option<i64>,
    region_code: Option<&str>,
) -> DeviceTreeDevice {
    DeviceTreeDevice {
        id,
        name: name.into(),
        device_tag: tag.into(),
        protocol: "Gb28181".into(),
        status: "Online".into(),
        is_online: true,
        has_stream: false,
        group_id,
        region_code: region_code.map(|s| s.to_string()),
    }
}

fn channel(id: i64, tag: &str, name: &str, device_tag: &str) -> DeviceTreeChannel {
    DeviceTreeChannel {
        id,
        device_tag: device_tag.into(),
        channel_tag: tag.into(),
        name: name.into(),
        status: "Online".into(),
    }
}

fn find<'a>(nodes: &'a [DeviceTreeNode], key: &str) -> Option<&'a DeviceTreeNode> {
    for n in nodes {
        if n.key == key {
            return Some(n);
        }
        if let Some(f) = find(&n.children, key) {
            return Some(f);
        }
    }
    None
}

/// 统计树里所有设备节点的数量（虚拟节点本身不算设备）。
fn count_device_nodes(nodes: &[DeviceTreeNode]) -> usize {
    nodes
        .iter()
        .map(|n| {
            if n.node_type == NODE_DEVICE {
                1
            } else {
                count_device_nodes(&n.children)
            }
        })
        .sum()
}

#[test]
fn all_devices_node_is_first_in_group_tree() {
    let g = vec![group(1, "东区", None)];
    let d = vec![
        device(1, "有组设备", "dev-1", Some(1), None),
        device(2, "无组设备", "dev-2", None, None),
    ];
    let tree = build_device_tree(&g, &[], &d, &[], None);
    let first = &tree.nodes[0];
    assert_eq!(first.node_type, ALL_TYPE);
    assert_eq!(first.key, ALL_KEY);
    assert_eq!(first.name, "全部设备");
    assert!(first.is_virtual);
    assert_eq!(first.count, Some(2));
    assert!(first.children.is_empty());
}

#[test]
fn all_devices_node_is_first_in_region_tree() {
    let r = vec![region("110101", "东城区", None)];
    let d = vec![device(1, "设备甲", "dev-1", None, Some("110101"))];
    let tree = build_device_tree(&[], &r, &d, &[], None);
    let first = &tree.region_nodes[0];
    assert_eq!(first.node_type, ALL_TYPE);
    assert_eq!(first.key, ALL_KEY);
    assert_eq!(first.name, "全部设备");
    assert!(first.is_virtual);
    assert_eq!(first.count, Some(1));
    assert!(first.children.is_empty());
}

#[test]
fn ungrouped_devices_are_grouped_under_ungrouped_node() {
    let g = vec![group(1, "东区", None)];
    let d = vec![
        device(1, "有组设备", "dev-1", Some(1), None),
        device(2, "无组设备", "dev-2", None, None),
    ];
    let tree = build_device_tree(&g, &[], &d, &[], None);
    let last = tree.nodes.last().expect("至少一个根节点");
    assert_eq!(last.node_type, UNGROUPED_TYPE);
    assert_eq!(last.key, UNGROUPED_KEY);
    assert_eq!(last.name, "未分组");
    assert!(last.is_virtual);
    assert_eq!(last.count, Some(1));
    assert_eq!(last.children.len(), 1);
    assert_eq!(last.children[0].node_type, NODE_DEVICE);
    assert_eq!(last.children[0].device_tag.as_deref(), Some("dev-2"));
    assert_eq!(last.children[0].group_id, None);
}

#[test]
fn region_tree_puts_devices_without_region_under_ungrouped_node() {
    let r = vec![region("110101", "东城区", None)];
    let d = vec![
        device(1, "有地区", "dev-1", None, Some("110101")),
        device(2, "无地区", "dev-2", None, None),
        device(3, "地区不存在", "dev-3", None, Some("999999")),
    ];
    let tree = build_device_tree(&[], &r, &d, &[], None);
    let ungrouped = find(&tree.region_nodes, UNGROUPED_KEY).expect("未分组节点");
    assert_eq!(ungrouped.node_type, UNGROUPED_TYPE);
    assert_eq!(ungrouped.count, Some(2));
    // 未归属地区的设备按名称排序：地区不存在 < 无地区
    let tags: Vec<&str> = ungrouped
        .children
        .iter()
        .filter_map(|c| c.device_tag.as_deref())
        .collect();
    assert_eq!(tags, vec!["dev-3", "dev-2"]);
    assert_eq!(ungrouped.children[0].region_code, None);
}

#[test]
fn ungrouped_node_exists_even_when_empty() {
    let g = vec![group(1, "东区", None)];
    let d = vec![device(1, "有组设备", "dev-1", Some(1), None)];
    let tree = build_device_tree(&g, &[], &d, &[], None);
    let last = tree.nodes.last().expect("至少一个根节点");
    assert_eq!(last.node_type, UNGROUPED_TYPE);
    assert_eq!(last.count, Some(0));
    assert!(last.children.is_empty());
}

#[test]
fn group_children_put_subgroups_before_devices() {
    let g = vec![group(1, "父组", None), group(2, "子组", Some(1))];
    let d = vec![
        device(1, "本级设备", "dev-1", Some(1), None),
        device(2, "子组设备", "dev-2", Some(2), None),
    ];
    let tree = build_device_tree(&g, &[], &d, &[], None);
    let root = find(&tree.nodes, "group:1").expect("父组节点");
    assert_eq!(root.children[0].node_type, NODE_GROUP);
    assert_eq!(root.children[0].name, "子组");
    assert_eq!(root.children[1].node_type, NODE_DEVICE);
    assert_eq!(root.children[1].name, "本级设备");
}

#[test]
fn group_count_is_recursive() {
    let g = vec![group(1, "父组", None), group(2, "子组", Some(1))];
    let d = vec![
        device(1, "a", "dev-1", Some(1), None),
        device(2, "b", "dev-2", Some(2), None),
    ];
    let tree = build_device_tree(&g, &[], &d, &[], None);
    let root = find(&tree.nodes, "group:1").expect("父组节点");
    assert_eq!(root.count, Some(2));
    assert_eq!(root.children[0].count, Some(1));
}

#[test]
fn empty_groups_are_kept_when_not_searching() {
    let g = vec![group(1, "空组", None)];
    let tree = build_device_tree(&g, &[], &[], &[], None);
    let empty = find(&tree.nodes, "group:1").expect("空组节点");
    assert_eq!(empty.name, "空组");
    assert_eq!(empty.count, Some(0));
}

#[test]
fn searching_prunes_groups_without_matches() {
    let g = vec![group(1, "命中组", None), group(2, "未命中组", None)];
    let d = vec![device(1, "cam-01", "dev-1", Some(1), None)];
    let tree = build_device_tree(&g, &[], &d, &[], Some("cam-01"));
    assert!(find(&tree.nodes, "group:1").is_some());
    assert!(find(&tree.nodes, "group:2").is_none());
}

#[test]
fn device_self_match_keeps_all_channels() {
    let g = vec![group(1, "组", None)];
    let d = vec![device(1, "大门摄像头", "dev-1", Some(1), None)];
    let c = vec![
        channel(1, "ch1", "通道一", "dev-1"),
        channel(2, "ch2", "通道二", "dev-1"),
    ];
    let tree = build_device_tree(&g, &[], &d, &c, Some("大门"));
    let dev = find(&tree.nodes, "device:dev-1").expect("设备命中");
    assert_eq!(dev.children.len(), 2);
}

#[test]
fn channel_match_keeps_only_matching_channels() {
    let g = vec![group(1, "组", None)];
    let d = vec![device(1, "摄像头", "dev-1", Some(1), None)];
    let c = vec![
        channel(1, "ch1", "通道一", "dev-1"),
        channel(2, "ch2", "通道二", "dev-1"),
    ];
    let tree = build_device_tree(&g, &[], &d, &c, Some("ch2"));
    let dev = find(&tree.nodes, "device:dev-1").expect("设备因通道命中而保留");
    assert_eq!(dev.children.len(), 1);
    assert_eq!(dev.children[0].key, "channel:dev-1:ch2");
    assert_eq!(dev.children[0].node_type, NODE_CHANNEL);
}

#[test]
fn search_matches_device_tag_and_channel_tag() {
    let g = vec![group(1, "组", None)];
    let d = vec![device(1, "摄像头", "dev-1", Some(1), None)];
    let c = vec![channel(1, "ch1", "通道一", "dev-1")];
    let by_tag = build_device_tree(&g, &[], &d, &c, Some("dev-1"));
    assert!(find(&by_tag.nodes, "device:dev-1").is_some());
    let by_channel_tag = build_device_tree(&g, &[], &d, &c, Some("ch1"));
    assert!(find(&by_channel_tag.nodes, "channel:dev-1:ch1").is_some());
}

#[test]
fn unmatched_devices_are_dropped_when_searching() {
    let g = vec![group(1, "组", None)];
    let d = vec![device(1, "无组设备", "dev-2", None, None)];
    let tree = build_device_tree(&g, &[], &d, &[], Some("不存在的关键字"));
    assert_eq!(count_device_nodes(&tree.nodes), 0);
}

#[test]
fn searching_keeps_all_node_with_total_count() {
    let g = vec![group(1, "组", None)];
    let d = vec![
        device(1, "cam-01", "dev-1", Some(1), None),
        device(2, "无组设备", "dev-2", None, None),
    ];
    let tree = build_device_tree(&g, &[], &d, &[], Some("cam-01"));
    let all = &tree.nodes[0];
    assert_eq!(all.node_type, ALL_TYPE);
    // 全部设备节点的计数始终是设备总数，不随搜索缩小
    assert_eq!(all.count, Some(2));
}

#[test]
fn searching_drops_ungrouped_node_without_matches() {
    let g = vec![group(1, "组", None)];
    let d = vec![
        device(1, "cam-01", "dev-1", Some(1), None),
        device(2, "无组设备", "dev-2", None, None),
    ];
    let tree = build_device_tree(&g, &[], &d, &[], Some("cam-01"));
    assert!(find(&tree.nodes, UNGROUPED_KEY).is_none());
}

#[test]
fn searching_keeps_ungrouped_node_with_matches() {
    let d = vec![device(1, "无组cam-01", "dev-1", None, None)];
    let tree = build_device_tree(&[], &[], &d, &[], Some("cam-01"));
    let ungrouped = find(&tree.nodes, UNGROUPED_KEY).expect("未分组节点保留");
    assert_eq!(ungrouped.count, Some(1));
    assert_eq!(ungrouped.children.len(), 1);
    assert_eq!(ungrouped.children[0].device_tag.as_deref(), Some("dev-1"));
}

#[test]
fn stats_reflect_visible_nodes() {
    let g = vec![group(1, "组", None)];
    let d = vec![device(1, "a", "dev-1", Some(1), None)];
    let c = vec![channel(1, "ch1", "通道一", "dev-1")];
    let tree = build_device_tree(&g, &[], &d, &c, None);
    assert_eq!(tree.stats.groups, 1);
    assert_eq!(tree.stats.devices, 1);
    assert_eq!(tree.stats.channels, 1);
}

#[test]
fn stats_ignore_virtual_nodes() {
    let g = vec![group(1, "组", None)];
    let r = vec![region("110101", "东城区", None)];
    let d = vec![
        device(1, "有组设备", "dev-1", Some(1), Some("110101")),
        device(2, "无组设备", "dev-2", None, None),
    ];
    let tree = build_device_tree(&g, &r, &d, &[], None);
    // 虚拟节点不计入分组数；设备仍只按分组树统计一次
    assert_eq!(tree.stats.groups, 1);
    assert_eq!(tree.stats.regions, 1);
    assert_eq!(tree.stats.devices, 2);
}

#[test]
fn orphan_group_devices_fall_into_ungrouped() {
    let g = vec![group(1, "仍在的分组", None)];
    let d = vec![
        device(1, "悬空设备", "dev-orphan", Some(999), None),
        device(2, "正常设备", "dev-ok", Some(1), None),
    ];
    let tree = build_device_tree(&g, &[], &d, &[], None);
    let ungrouped = find(&tree.nodes, UNGROUPED_KEY).expect("未分组节点");
    assert_eq!(ungrouped.children.len(), 1);
    assert_eq!(
        ungrouped.children[0].device_tag.as_deref(),
        Some("dev-orphan")
    );
    assert_eq!(ungrouped.children[0].group_id, None);
    assert_eq!(tree.stats.devices, 2);
}

#[test]
fn device_nodes_carry_status_and_tags() {
    let g = vec![group(1, "组", None)];
    let d = vec![device(1, "a", "dev-1", Some(1), None)];
    let tree = build_device_tree(&g, &[], &d, &[], None);
    let dev = find(&tree.nodes, "device:dev-1").unwrap();
    assert_eq!(dev.node_type, NODE_DEVICE);
    assert_eq!(dev.device_tag.as_deref(), Some("dev-1"));
    assert_eq!(dev.status.as_deref(), Some("Online"));
    assert_eq!(dev.group_id, Some(1));
    assert_eq!(dev.channel_tag, None);
}

#[test]
fn region_tree_places_devices_under_region() {
    let r = vec![region("110101", "东城区", None)];
    let d = vec![device(1, "设备甲", "dev-1", None, Some("110101"))];
    let tree = build_device_tree(&[], &r, &d, &[], None);
    let region_node = find(&tree.region_nodes, "region:110101").expect("地区节点");
    assert_eq!(region_node.node_type, NODE_REGION);
    assert_eq!(region_node.region_code.as_deref(), Some("110101"));
    assert_eq!(region_node.count, Some(1));
    assert_eq!(region_node.children.len(), 1);
    assert_eq!(region_node.children[0].node_type, NODE_DEVICE);
    assert_eq!(region_node.children[0].device_tag.as_deref(), Some("dev-1"));
}

#[test]
fn region_tree_nests_sub_regions_before_devices() {
    let r = vec![
        region("11", "北京市", None),
        region("110101", "东城区", Some("11")),
    ];
    let d = vec![device(1, "本级设备", "dev-1", None, Some("11"))];
    let tree = build_device_tree(&[], &r, &d, &[], None);
    let root = find(&tree.region_nodes, "region:11").expect("地区节点");
    assert_eq!(root.children[0].node_type, NODE_REGION);
    assert_eq!(root.children[0].name, "东城区");
    assert_eq!(root.children[1].node_type, NODE_DEVICE);
    assert_eq!(root.children[1].name, "本级设备");
}

#[test]
fn region_count_is_recursive() {
    let r = vec![
        region("11", "北京市", None),
        region("110101", "东城区", Some("11")),
    ];
    let d = vec![device(1, "设备甲", "dev-1", None, Some("110101"))];
    let tree = build_device_tree(&[], &r, &d, &[], None);
    let root = find(&tree.region_nodes, "region:11").expect("地区节点");
    assert_eq!(root.count, Some(1));
    assert_eq!(root.children[0].count, Some(1));
}

#[test]
fn device_node_carries_region_code() {
    let r = vec![region("110101", "东城区", None)];
    let d = vec![device(1, "设备甲", "dev-1", None, Some("110101"))];
    let tree = build_device_tree(&[], &r, &d, &[], None);
    let dev = find(&tree.region_nodes, "device:dev-1").unwrap();
    assert_eq!(dev.region_code.as_deref(), Some("110101"));
}

#[test]
fn region_tree_and_group_tree_show_the_same_device() {
    let g = vec![group(1, "组", None)];
    let r = vec![region("110101", "东城区", None)];
    let d = vec![device(1, "设备甲", "dev-1", Some(1), Some("110101"))];
    let tree = build_device_tree(&g, &r, &d, &[], None);
    assert!(find(&tree.nodes, "device:dev-1").is_some());
    assert!(find(&tree.region_nodes, "device:dev-1").is_some());
}

#[test]
fn searching_prunes_regions_without_matches() {
    let r = vec![
        region("110101", "东城区", None),
        region("110102", "西城区", None),
    ];
    let d = vec![device(1, "cam-01", "dev-1", None, Some("110101"))];
    let tree = build_device_tree(&[], &r, &d, &[], Some("cam-01"));
    assert!(find(&tree.region_nodes, "region:110101").is_some());
    assert!(find(&tree.region_nodes, "region:110102").is_none());
}

#[test]
fn region_tree_keeps_channels_under_device() {
    let r = vec![region("110101", "东城区", None)];
    let d = vec![device(1, "设备甲", "dev-1", None, Some("110101"))];
    let c = vec![channel(1, "ch1", "通道一", "dev-1")];
    let tree = build_device_tree(&[], &r, &d, &c, None);
    let dev = find(&tree.region_nodes, "device:dev-1").unwrap();
    assert_eq!(dev.children.len(), 1);
    assert_eq!(dev.children[0].node_type, NODE_CHANNEL);
}

/// 响应 JSON 的字段名是前后端契约，前端按 `nodes` / `region_nodes` 取两棵树，
/// 并按 `type` 识别「全部设备」「未分组」两个虚拟节点。
#[test]
fn response_json_shape_matches_frontend_contract() {
    let g = vec![group(1, "组", None)];
    let r = vec![region("110101", "东城区", None)];
    let d = vec![
        device(1, "有组设备", "dev-1", Some(1), Some("110101")),
        device(2, "无组设备", "dev-2", None, None),
    ];
    let tree = build_device_tree(&g, &r, &d, &[], None);
    let json = serde_json::to_value(&tree).expect("序列化");

    // 分组树：全部设备 → 分组 → 未分组
    assert_eq!(json["nodes"][0]["type"], "all");
    assert_eq!(json["nodes"][0]["virtual"], true);
    assert_eq!(json["nodes"][0]["name"], "全部设备");
    assert_eq!(json["nodes"][0]["children"].as_array().unwrap().len(), 0);
    assert_eq!(json["nodes"][1]["type"], "group");
    assert_eq!(json["nodes"][2]["type"], "ungrouped");
    assert_eq!(json["nodes"][2]["virtual"], true);
    assert_eq!(json["nodes"][2]["name"], "未分组");
    // 没有分组的设备挂在未分组节点下，且不带 group_id 字段
    assert_eq!(json["nodes"][2]["children"][0]["key"], "device:dev-2");
    assert!(json["nodes"][2]["children"][0].get("group_id").is_none());

    // 地区树：全部设备 → 地区 → 未分组
    assert_eq!(json["region_nodes"][0]["type"], "all");
    assert_eq!(json["region_nodes"][1]["type"], "region");
    assert_eq!(json["region_nodes"][1]["region_code"], "110101");
    assert_eq!(
        json["region_nodes"][1]["children"][0]["key"],
        "device:dev-1"
    );
    assert_eq!(json["region_nodes"][2]["type"], "ungrouped");
    assert_eq!(
        json["region_nodes"][2]["children"][0]["key"],
        "device:dev-2"
    );
}

/// 普通节点不携带 virtual 标志，前端只靠它识别虚拟节点。
#[test]
fn regular_nodes_have_no_virtual_flag() {
    let g = vec![group(1, "组", None)];
    let d = vec![device(1, "a", "dev-1", Some(1), None)];
    let tree = build_device_tree(&g, &[], &d, &[], None);
    let json = serde_json::to_value(&tree).expect("序列化");
    assert!(json["nodes"][1].get("virtual").is_none());
    assert!(json["nodes"][1]["children"][0].get("virtual").is_none());
}
