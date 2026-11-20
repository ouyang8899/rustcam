//! 层级过滤展开：把「某个分组」翻译成「它自己 + 全部子孙」的标识列表。
//!
//! 设备列表按自定义分组过滤时，用户点的是树上的任意一层节点，
//! 期望看到的是这一层及其下属所有层级的设备。因此过滤条件不能只做
//! 等值匹配，必须先把分组展开成一张 id 清单，再交给 SQL 的 `IN`。
//!
//! 地区不走这里：行政区划编码本身就是「父编码 + 扩展码」，
//! 用 `LIKE '父编码%'` 就能覆盖全部子地区。
//!
//! 这里刻意是纯函数：输入是扁平的 `(id, 父 id)` 列表，
//! 不依赖数据库，方便单元测试。

use std::collections::HashMap;

/// 展开分组：返回 `group_id` 以及它下面所有子孙分组的 id。
///
/// 未知分组只返回它自己（这样 SQL 里的 `IN` 列表永远非空，
/// 且不会误命中任何设备）。父子关系成环时也不会死循环。
pub fn expand_group_ids(groups: &[(i64, Option<i64>)], group_id: i64) -> Vec<i64> {
    let mut children: HashMap<Option<i64>, Vec<i64>> = HashMap::new();
    for (id, parent_id) in groups {
        children.entry(*parent_id).or_default().push(*id);
    }

    let mut result: Vec<i64> = Vec::new();
    let mut stack = vec![group_id];
    while let Some(id) = stack.pop() {
        if result.contains(&id) {
            continue;
        }
        result.push(id);
        if let Some(kids) = children.get(&Some(id)) {
            stack.extend(kids.iter().copied());
        }
    }
    result
}
