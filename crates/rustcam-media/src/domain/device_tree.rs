//! 设备树的领域模型与纯构建函数。
//!
//! 这里刻意不依赖数据库：输入是四组扁平列表（分组、地区、设备、通道），
//! 输出是两棵树：
//! - 分组树 `分组 → 设备 → 通道`
//! - 地区树 `地区 → 设备 → 通道`
//!
//! HTTP 层负责查库与用内存缓存覆盖实时状态，组装规则全部集中在这里，
//! 方便脱离数据库做单元测试。
//!
//! 组装规则：
//! - 分组按 `(sort_order, name)` 排序，地区按 `name` 排序，
//!   设备按 `(name, device_tag)`，通道按 `id`
//! - 组/地区内顺序：子分组（子地区）在前，本级设备在后
//! - 两棵树的根都带两个后端合成的虚拟节点：
//!   第一个是「全部设备」（不含任何设备，count 是设备总数），
//!   最后一个是「未分组」（装着没有归属分组/地区的设备）
//! - 搜索时：设备自身命中保留其全部通道；仅通道命中只保留命中通道；
//!   无命中的分组与地区被剪掉；「全部设备」始终保留，
//!   「未分组」没有命中设备时隐藏

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const NODE_GROUP: &str = "group";
pub const NODE_DEVICE: &str = "device";
pub const NODE_CHANNEL: &str = "channel";
pub const NODE_REGION: &str = "region";
/// 虚拟节点：「全部设备」，固定在树根第一位，不挂设备。
pub const NODE_ALL: &str = "all";
/// 虚拟节点：「未分组」，固定在树根最后一位，装未归属分组/地区的设备。
pub const NODE_UNGROUPED: &str = "ungrouped";

/// 虚拟节点「全部设备」的显示名。
pub const LABEL_ALL_DEVICES: &str = "全部设备";
/// 虚拟节点「未分组」的显示名。
pub const LABEL_UNGROUPED: &str = "未分组";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceTreeGroup {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub sort_order: i32,
}

/// 地区（行政区划）扁平节点，`parent_code` 指向父地区。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceTreeRegion {
    pub code: String,
    pub name: String,
    pub parent_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceTreeDevice {
    pub id: i64,
    pub name: String,
    pub device_tag: String,
    pub protocol: String,
    pub status: String,
    pub is_online: bool,
    pub has_stream: bool,
    pub group_id: Option<i64>,
    pub region_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceTreeChannel {
    pub id: i64,
    pub device_tag: String,
    pub channel_tag: String,
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTreeNode {
    pub key: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub id: i64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<i64>,
    pub count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_online: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<i64>,
    /// 地区节点：自身地区编码；设备节点：所属地区编码。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region_code: Option<String>,
    /// 虚拟节点标记：后端合成的「全部设备」「未分组」，前端靠它识别。
    #[serde(rename = "virtual", default, skip_serializing_if = "is_false")]
    pub is_virtual: bool,
    pub children: Vec<DeviceTreeNode>,
}

/// `virtual` 只在虚拟节点上出现，普通节点不带这个字段。
fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceTreeStats {
    pub groups: usize,
    pub regions: usize,
    pub devices: usize,
    pub channels: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTreeResponse {
    /// 分组树：分组 → 设备 → 通道，未分组设备平铺在根末尾。
    pub nodes: Vec<DeviceTreeNode>,
    /// 地区树：地区 → 设备 → 通道，未分配地区设备平铺在根末尾。
    pub region_nodes: Vec<DeviceTreeNode>,
    pub stats: DeviceTreeStats,
}

fn contains(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(needle)
}

/// 设备自身是否命中关键字（名称或 tag）。
fn device_matches(device: &DeviceTreeDevice, needle: &str) -> bool {
    contains(&device.name, needle) || contains(&device.device_tag, needle)
}

/// 通道是否命中关键字（名称、通道 tag 或所属设备 tag）。
fn channel_matches(channel: &DeviceTreeChannel, needle: &str) -> bool {
    contains(&channel.name, needle)
        || contains(&channel.channel_tag, needle)
        || contains(&channel.device_tag, needle)
}

fn group_key(id: i64) -> String {
    format!("group:{}", id)
}

fn device_key(tag: &str) -> String {
    format!("device:{}", tag)
}

fn channel_key(device_tag: &str, channel_tag: &str) -> String {
    format!("channel:{}:{}", device_tag, channel_tag)
}

fn region_key(code: &str) -> String {
    format!("region:{}", code)
}

/// 统计一层子节点里的设备数（分组/地区节点递归展开，设备节点计 1）。
fn count_devices_in(children: &[DeviceTreeNode]) -> u32 {
    children
        .iter()
        .map(|c| {
            if c.node_type == NODE_DEVICE {
                1
            } else {
                count_devices_in(&c.children)
            }
        })
        .sum()
}

fn count_stats(nodes: &[DeviceTreeNode], stats: &mut DeviceTreeStats) {
    for node in nodes {
        match node.node_type.as_str() {
            NODE_GROUP => stats.groups += 1,
            NODE_DEVICE => {
                stats.devices += 1;
                stats.channels += node.children.len();
            }
            _ => {}
        }
        count_stats(&node.children, stats);
    }
}

/// 统计地区树里的地区节点数量。
fn count_region_nodes(nodes: &[DeviceTreeNode]) -> usize {
    nodes
        .iter()
        .map(|n| {
            if n.node_type == NODE_REGION {
                1 + count_region_nodes(&n.children)
            } else {
                count_region_nodes(&n.children)
            }
        })
        .sum()
}

/// 构建设备树：分组树 + 地区树。
///
/// - `search` 为 `None` 或空白时返回完整树，空分组/空地区保留
/// - 搜索时：设备自身命中则保留其全部通道；仅通道命中则只保留命中通道；
///   无命中的分组与地区被剪掉
pub fn build_device_tree(
    groups: &[DeviceTreeGroup],
    regions: &[DeviceTreeRegion],
    devices: &[DeviceTreeDevice],
    channels: &[DeviceTreeChannel],
    search: Option<&str>,
) -> DeviceTreeResponse {
    let needle = search
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());

    // 通道按设备归桶
    let mut channels_by_device: HashMap<&str, Vec<&DeviceTreeChannel>> = HashMap::new();
    for channel in channels {
        channels_by_device
            .entry(channel.device_tag.as_str())
            .or_default()
            .push(channel);
    }
    for list in channels_by_device.values_mut() {
        list.sort_by_key(|c| c.id);
    }

    // 已删除/悬空分组、地区的设备必须落到「无分组 / 无地区」，
    // 否则会从树里彻底消失。
    let known_groups: std::collections::HashSet<i64> = groups.iter().map(|g| g.id).collect();
    let known_regions: std::collections::HashSet<&str> =
        regions.iter().map(|r| r.code.as_str()).collect();

    // 逐设备决定是否保留、以及保留哪些通道
    let mut group_devices: HashMap<Option<i64>, Vec<DeviceTreeNode>> = HashMap::new();
    let mut region_devices: HashMap<Option<String>, Vec<DeviceTreeNode>> = HashMap::new();
    for device in devices {
        let group_id = device.group_id.filter(|id| known_groups.contains(id));
        let region_code = device
            .region_code
            .clone()
            .filter(|code| !code.is_empty() && known_regions.contains(code.as_str()));

        let empty: &[&DeviceTreeChannel] = &[];
        let own_channels = channels_by_device
            .get(device.device_tag.as_str())
            .map(|v| v.as_slice())
            .unwrap_or(empty);

        let kept_channels: Vec<&DeviceTreeChannel> = match &needle {
            None => own_channels.to_vec(),
            Some(n) => {
                if device_matches(device, n) {
                    own_channels.to_vec()
                } else {
                    own_channels
                        .iter()
                        .copied()
                        .filter(|c| channel_matches(c, n))
                        .collect()
                }
            }
        };

        let keep = match &needle {
            None => true,
            Some(n) => device_matches(device, n) || !kept_channels.is_empty(),
        };
        if !keep {
            continue;
        }

        let children = kept_channels
            .iter()
            .map(|c| DeviceTreeNode {
                key: channel_key(&c.device_tag, &c.channel_tag),
                node_type: NODE_CHANNEL.to_string(),
                id: c.id,
                name: c.name.clone(),
                parent_id: None,
                count: None,
                device_tag: Some(c.device_tag.clone()),
                channel_tag: Some(c.channel_tag.clone()),
                protocol: None,
                status: Some(c.status.clone()),
                is_online: None,
                has_stream: None,
                group_id: None,
                region_code: None,
                is_virtual: false,
                children: Vec::new(),
            })
            .collect();

        let node = DeviceTreeNode {
            key: device_key(&device.device_tag),
            node_type: NODE_DEVICE.to_string(),
            id: device.id,
            name: device.name.clone(),
            parent_id: None,
            count: None,
            device_tag: Some(device.device_tag.clone()),
            channel_tag: None,
            protocol: Some(device.protocol.clone()),
            status: Some(device.status.clone()),
            is_online: Some(device.is_online),
            has_stream: Some(device.has_stream),
            group_id,
            region_code: region_code.clone(),
            is_virtual: false,
            children,
        };

        group_devices
            .entry(group_id)
            .or_default()
            .push(node.clone());
        region_devices.entry(region_code).or_default().push(node);
    }

    // 本级设备排序：名称 → tag
    for list in group_devices.values_mut() {
        sort_devices(list);
    }
    for list in region_devices.values_mut() {
        sort_devices(list);
    }

    let (mut roots, loose_group_devices) = build_group_tree(groups, &group_devices);
    let (mut region_roots, loose_region_devices) = build_region_tree(regions, &region_devices);

    // 搜索时剪掉没有命中设备的分组与地区
    if needle.is_some() {
        fn prune(nodes: &mut Vec<DeviceTreeNode>) {
            nodes.retain_mut(|node| {
                prune(&mut node.children);
                if node.node_type == NODE_GROUP || node.node_type == NODE_REGION {
                    node.count = Some(count_devices_in(&node.children));
                    node.count.unwrap_or(0) > 0
                } else {
                    true
                }
            });
        }
        prune(&mut roots);
        prune(&mut region_roots);
    }

    // 两棵树都补上虚拟节点：首位「全部设备」，末位「未分组」。
    // 虚拟节点不参与剪枝，但搜索时「未分组」没有命中设备就隐藏。
    let total_devices = devices.len() as u32;
    let mut all_nodes = Vec::with_capacity(roots.len() + 2);
    all_nodes.push(all_devices_node(total_devices));
    all_nodes.extend(roots);
    if let Some(node) = ungrouped_node(loose_group_devices, needle.is_some()) {
        all_nodes.push(node);
    }
    let mut all_region_nodes = Vec::with_capacity(region_roots.len() + 2);
    all_region_nodes.push(all_devices_node(total_devices));
    all_region_nodes.extend(region_roots);
    if let Some(node) = ungrouped_node(loose_region_devices, needle.is_some()) {
        all_region_nodes.push(node);
    }
    let roots = all_nodes;
    let region_roots = all_region_nodes;

    let mut stats = DeviceTreeStats::default();
    // 设备与通道在两棵树里各出现一次，统计只取分组树，避免重复计数。
    count_stats(&roots, &mut stats);
    stats.regions = count_region_nodes(&region_roots);

    DeviceTreeResponse {
        nodes: roots,
        region_nodes: region_roots,
        stats,
    }
}

/// 虚拟节点「全部设备」：不挂设备，count 是设备总数。
fn all_devices_node(total_devices: u32) -> DeviceTreeNode {
    DeviceTreeNode {
        key: NODE_ALL.to_string(),
        node_type: NODE_ALL.to_string(),
        id: 0,
        name: LABEL_ALL_DEVICES.to_string(),
        parent_id: None,
        count: Some(total_devices),
        device_tag: None,
        channel_tag: None,
        protocol: None,
        status: None,
        is_online: None,
        has_stream: None,
        group_id: None,
        region_code: None,
        is_virtual: true,
        children: Vec::new(),
    }
}

/// 虚拟节点「未分组」：装着没有归属分组/地区的设备。
///
/// 搜索时若一台都没命中，返回 `None` 让调用方跳过这个节点。
fn ungrouped_node(devices: Vec<DeviceTreeNode>, searching: bool) -> Option<DeviceTreeNode> {
    if searching && devices.is_empty() {
        return None;
    }
    let count = count_devices_in(&devices);
    Some(DeviceTreeNode {
        key: NODE_UNGROUPED.to_string(),
        node_type: NODE_UNGROUPED.to_string(),
        id: 0,
        name: LABEL_UNGROUPED.to_string(),
        parent_id: None,
        count: Some(count),
        device_tag: None,
        channel_tag: None,
        protocol: None,
        status: None,
        is_online: None,
        has_stream: None,
        group_id: None,
        region_code: None,
        is_virtual: true,
        children: devices,
    })
}

fn sort_devices(devices: &mut [DeviceTreeNode]) {
    devices.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.device_tag.cmp(&b.device_tag))
    });
}

/// 分组树：`分组 → 设备`。
///
/// 返回 `(根节点, 没有分组的设备)`：后者不铺在根上，
/// 交给「未分组」虚拟节点承载。
fn build_group_tree(
    groups: &[DeviceTreeGroup],
    device_children: &HashMap<Option<i64>, Vec<DeviceTreeNode>>,
) -> (Vec<DeviceTreeNode>, Vec<DeviceTreeNode>) {
    // 分组索引：parent_id → 子分组下标
    let mut group_children: HashMap<Option<i64>, Vec<usize>> = HashMap::new();
    let mut sorted_group_indices: Vec<usize> = (0..groups.len()).collect();
    sorted_group_indices.sort_by(|a, b| {
        groups[*a]
            .sort_order
            .cmp(&groups[*b].sort_order)
            .then_with(|| groups[*a].name.cmp(&groups[*b].name))
    });
    for idx in &sorted_group_indices {
        group_children
            .entry(groups[*idx].parent_id)
            .or_default()
            .push(*idx);
    }

    fn build_group_node(
        idx: usize,
        groups: &[DeviceTreeGroup],
        group_children: &HashMap<Option<i64>, Vec<usize>>,
        device_children: &HashMap<Option<i64>, Vec<DeviceTreeNode>>,
        visiting: &mut Vec<i64>,
    ) -> Option<DeviceTreeNode> {
        let group = &groups[idx];
        if visiting.contains(&group.id) {
            return None;
        }
        visiting.push(group.id);

        let mut children: Vec<DeviceTreeNode> = Vec::new();
        if let Some(sub_indices) = group_children.get(&Some(group.id)) {
            for sub_idx in sub_indices {
                if let Some(node) =
                    build_group_node(*sub_idx, groups, group_children, device_children, visiting)
                {
                    children.push(node);
                }
            }
        }
        if let Some(devices) = device_children.get(&Some(group.id)) {
            children.extend(devices.iter().cloned());
        }

        visiting.pop();

        let count = count_devices_in(&children);

        Some(DeviceTreeNode {
            key: group_key(group.id),
            node_type: NODE_GROUP.to_string(),
            id: group.id,
            name: group.name.clone(),
            parent_id: group.parent_id,
            count: Some(count),
            device_tag: None,
            channel_tag: None,
            protocol: None,
            status: None,
            is_online: None,
            has_stream: None,
            group_id: None,
            region_code: None,
            is_virtual: false,
            children,
        })
    }

    let mut roots: Vec<DeviceTreeNode> = Vec::new();
    if let Some(root_indices) = group_children.get(&None) {
        for root_idx in root_indices {
            if let Some(node) = build_group_node(
                *root_idx,
                groups,
                &group_children,
                device_children,
                &mut Vec::new(),
            ) {
                roots.push(node);
            }
        }
    }

    // 没有分组的设备单独返回，交给「未分组」节点承载
    let loose = device_children.get(&None).cloned().unwrap_or_default();

    (roots, loose)
}

/// 地区树：`地区 → 设备`。
///
/// 返回 `(根节点, 没有地区的设备)`：后者不铺在根上，
/// 交给「未分组」虚拟节点承载。
fn build_region_tree(
    regions: &[DeviceTreeRegion],
    device_children: &HashMap<Option<String>, Vec<DeviceTreeNode>>,
) -> (Vec<DeviceTreeNode>, Vec<DeviceTreeNode>) {
    let mut region_children: HashMap<Option<String>, Vec<usize>> = HashMap::new();
    let mut sorted_region_indices: Vec<usize> = (0..regions.len()).collect();
    sorted_region_indices.sort_by(|a, b| {
        regions[*a]
            .name
            .cmp(&regions[*b].name)
            .then_with(|| regions[*a].code.cmp(&regions[*b].code))
    });
    for idx in &sorted_region_indices {
        region_children
            .entry(regions[*idx].parent_code.clone())
            .or_default()
            .push(*idx);
    }

    fn build_region_node(
        idx: usize,
        regions: &[DeviceTreeRegion],
        region_children: &HashMap<Option<String>, Vec<usize>>,
        device_children: &HashMap<Option<String>, Vec<DeviceTreeNode>>,
        visiting: &mut Vec<String>,
    ) -> Option<DeviceTreeNode> {
        let region = &regions[idx];
        if visiting.contains(&region.code) {
            return None;
        }
        visiting.push(region.code.clone());

        let mut children: Vec<DeviceTreeNode> = Vec::new();
        if let Some(sub_indices) = region_children.get(&Some(region.code.clone())) {
            for sub_idx in sub_indices {
                if let Some(node) = build_region_node(
                    *sub_idx,
                    regions,
                    region_children,
                    device_children,
                    visiting,
                ) {
                    children.push(node);
                }
            }
        }
        if let Some(devices) = device_children.get(&Some(region.code.clone())) {
            children.extend(devices.iter().cloned());
        }

        visiting.pop();

        let count = count_devices_in(&children);

        Some(DeviceTreeNode {
            key: region_key(&region.code),
            node_type: NODE_REGION.to_string(),
            id: 0,
            name: region.name.clone(),
            parent_id: None,
            count: Some(count),
            device_tag: None,
            channel_tag: None,
            protocol: None,
            status: None,
            is_online: None,
            has_stream: None,
            group_id: None,
            region_code: Some(region.code.clone()),
            is_virtual: false,
            children,
        })
    }

    let mut roots: Vec<DeviceTreeNode> = Vec::new();
    if let Some(root_indices) = region_children.get(&None) {
        for root_idx in root_indices {
            if let Some(node) = build_region_node(
                *root_idx,
                regions,
                &region_children,
                device_children,
                &mut Vec::new(),
            ) {
                roots.push(node);
            }
        }
    }

    // 未分配地区的设备单独返回，交给「未分组」节点承载
    let loose = device_children.get(&None).cloned().unwrap_or_default();

    (roots, loose)
}
