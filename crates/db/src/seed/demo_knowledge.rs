//! Generated Markdown knowledge docs for the demo factory.
//!
//! A-scheme storage: one `resources` row per document, Markdown in
//! `content`, `thing_id` points at a device/space/building/line. The docs are
//! intentionally generated from the live thing model (properties, actions,
//! location) so the demo has complete, searchable manuals without a separate
//! file tree. Devices get a manual + inspection/fault SOP; spaces get an area
//! guide. `INSERT OR IGNORE` keeps user edits and makes startup idempotent.

use std::collections::HashMap;

use sqlx::{FromRow, SqlitePool};

use crate::Db;

const WORKSPACE_ID: &str = "ws-default-001";
const ROOT_ID: &str = "site-demo-factory";

#[derive(Debug, FromRow)]
struct ThingRow {
    id: String,
    name: String,
    display_name: Option<String>,
    thing_type: String,
    category: Option<String>,
    address: Option<String>,
    description: Option<String>,
    parent_id: Option<String>,
    driver_name: Option<String>,
    protocol_type: Option<String>,
}

#[derive(Debug, FromRow)]
struct PropertyRow {
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    data_type: Option<String>,
    unit: Option<String>,
    min_value: Option<f64>,
    max_value: Option<f64>,
    is_read_only: i64,
}

#[derive(Debug, FromRow)]
struct ActionRow {
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    parameters: Option<String>,
}

pub(crate) async fn seed(db: &Db) -> Result<(), sqlx::Error> {
    let pool = db.pool();
    let all: Vec<ThingRow> = sqlx::query_as(
        "SELECT id, name, display_name, thing_type, category, address, description, parent_id, \
                driver_name, protocol_type \
         FROM things WHERE workspace_id = ?",
    )
    .bind(WORKSPACE_ID)
    .fetch_all(pool)
    .await?;
    let by_id: HashMap<&str, &ThingRow> = all.iter().map(|thing| (thing.id.as_str(), thing)).collect();
    if !by_id.contains_key(ROOT_ID) {
        return Ok(());
    }

    let mut children: HashMap<&str, Vec<&ThingRow>> = HashMap::new();
    for thing in &all {
        if let Some(parent) = thing.parent_id.as_deref() {
            children.entry(parent).or_default().push(thing);
        }
    }
    for list in children.values_mut() {
        list.sort_by(|a, b| a.id.cmp(&b.id));
    }

    // Breadth-first from the demo root so an ancestor is always generated
    // before its descendants (and user things outside the root are ignored).
    let mut order: Vec<&ThingRow> = Vec::new();
    let mut queue: Vec<&str> = vec![ROOT_ID];
    while let Some(id) = queue.pop() {
        let Some(thing) = by_id.get(id) else { continue };
        order.push(thing);
        if let Some(kids) = children.get(id) {
            for kind in kids.iter().rev() {
                queue.push(kind.id.as_str());
            }
        }
    }

    for thing in order {
        let tags = load_tags(pool, &thing.id).await?;
        let breadcrumb = breadcrumb(thing, &by_id);
        let location = breadcrumb
            .last()
            .cloned()
            .unwrap_or_else(|| display_name(thing).to_string());
        let mut doc_tags = vec![
            if thing.thing_type == "device" {
                "设备文档".to_string()
            } else {
                "区域文档".to_string()
            },
            thing.category.clone().unwrap_or_else(|| "未分类".to_string()),
            location,
        ];
        doc_tags.extend(tags);

        if thing.thing_type == "device" {
            let properties = load_properties(pool, &thing.id).await?;
            let actions = load_actions(pool, &thing.id).await?;
            let manual = device_manual(thing, &breadcrumb, &properties, &actions);
            insert_doc(
                pool,
                thing,
                "manual",
                &format!("{} 设备档案", display_name(thing)),
                "设备铭牌、属性量程、可用动作与运维要点",
                &manual,
                &doc_tags,
                "manual",
            )
            .await?;

            let sop = device_sop(thing, &breadcrumb, &properties);
            insert_doc(
                pool,
                thing,
                "sop",
                &format!("{} 巡检与故障处置", display_name(thing)),
                "巡检清单、告警处置、常见故障与升级条件",
                &sop,
                &doc_tags,
                "sop",
            )
            .await?;
        } else {
            let kids = children.get(thing.id.as_str()).cloned().unwrap_or_default();
            let area = area_guide(thing, &breadcrumb, &kids);
            insert_doc(
                pool,
                thing,
                "area",
                &format!("{} 区域说明与巡检路线", display_name(thing)),
                "区域概况、下辖设备、环境安全要求与巡检路线",
                &area,
                &doc_tags,
                "area",
            )
            .await?;
        }
    }
    Ok(())
}

async fn load_tags(pool: &SqlitePool, thing_id: &str) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT t.name FROM tags t JOIN tag_bindings b ON b.tag_id = t.id \
         WHERE b.target_id = ? AND b.target_type = 'thing' ORDER BY t.name",
    )
    .bind(thing_id)
    .fetch_all(pool)
    .await
}

async fn load_properties(pool: &SqlitePool, thing_id: &str) -> Result<Vec<PropertyRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT name, display_name, description, data_type, unit, min_value, max_value, is_read_only \
         FROM thing_properties WHERE thing_id = ? ORDER BY name",
    )
    .bind(thing_id)
    .fetch_all(pool)
    .await
}

async fn load_actions(pool: &SqlitePool, thing_id: &str) -> Result<Vec<ActionRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT name, display_name, description, parameters FROM thing_actions \
         WHERE thing_id = ? ORDER BY name",
    )
    .bind(thing_id)
    .fetch_all(pool)
    .await
}

fn display_name(thing: &ThingRow) -> &str {
    thing.display_name.as_deref().unwrap_or(&thing.name)
}

fn breadcrumb(thing: &ThingRow, by_id: &HashMap<&str, &ThingRow>) -> Vec<String> {
    let mut chain = Vec::new();
    let mut current = Some(thing);
    while let Some(node) = current {
        if node.id != thing.id {
            chain.push(display_name(node).to_string());
        }
        current = node.parent_id.as_deref().and_then(|id| by_id.get(id).copied());
    }
    chain.reverse();
    chain
}

fn md_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn range_cell(property: &PropertyRow) -> String {
    match (property.min_value, property.max_value) {
        (Some(min), Some(max)) => format!("{min} ~ {max}"),
        (Some(min), None) => format!("≥ {min}"),
        (None, Some(max)) => format!("≤ {max}"),
        (None, None) => "—".to_string(),
    }
}

fn parameter_text(parameters: Option<&str>) -> String {
    let Some(raw) = parameters else {
        return "—".to_string();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return md_cell(raw);
    };
    if let Some(specs) = value.as_array() {
        if specs.is_empty() {
            return "—".to_string();
        }
        return specs
            .iter()
            .filter_map(|spec| {
                let name = spec.get("name").and_then(|v| v.as_str())?;
                let ty = spec.get("type").and_then(|v| v.as_str()).unwrap_or("string");
                let required = spec.get("required").and_then(|v| v.as_bool()).unwrap_or(false);
                Some(format!("{name}:{ty}{}", if required { "(必填)" } else { "" }))
            })
            .collect::<Vec<_>>()
            .join(", ");
    }
    if value.as_object().is_some_and(|object| object.is_empty()) {
        "—".to_string()
    } else {
        md_cell(&value.to_string())
    }
}

fn device_manual(thing: &ThingRow, breadcrumb: &[String], properties: &[PropertyRow], actions: &[ActionRow]) -> String {
    let category = thing.category.as_deref().unwrap_or("未分类");
    let location = if breadcrumb.is_empty() {
        thing.address.as_deref().unwrap_or("未配置位置")
    } else {
        &breadcrumb.join(" / ")
    };
    let mut out = String::new();
    out.push_str(&format!("# {} 设备档案\n\n", display_name(thing)));
    out.push_str(&format!(
        "> 设备 ID：`{}` · 位置：{} · 类型：{}\n\n",
        thing.id, location, category
    ));
    out.push_str("## 1. 基本信息\n\n| 项目 | 值 |\n|---|---|\n");
    out.push_str(&format!("| 设备名称 | {} |\n", md_cell(display_name(thing))));
    out.push_str(&format!("| 物类型 | {} |\n", md_cell(&thing.thing_type)));
    out.push_str(&format!("| 分类 | {} |\n", md_cell(category)));
    out.push_str(&format!(
        "| 安装位置 | {} |\n",
        md_cell(thing.address.as_deref().unwrap_or("—"))
    ));
    out.push_str(&format!(
        "| 通信协议 | {} |\n",
        md_cell(thing.protocol_type.as_deref().unwrap_or("—"))
    ));
    out.push_str(&format!(
        "| 驱动 | {} |\n",
        md_cell(thing.driver_name.as_deref().unwrap_or("—"))
    ));
    if let Some(description) = &thing.description {
        out.push_str(&format!("| 用途说明 | {} |\n", md_cell(description)));
    }
    out.push_str("\n## 2. 属性与量程\n\n");
    if properties.is_empty() {
        out.push_str("该设备暂未声明属性。\n\n");
    } else {
        out.push_str("| 属性 | 说明 | 类型 | 单位 | 范围 | 读写 |\n|---|---|---|---|---|---|\n");
        for property in properties {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                md_cell(&property.name),
                md_cell(
                    property
                        .description
                        .as_deref()
                        .or(property.display_name.as_deref())
                        .unwrap_or("—")
                ),
                md_cell(property.data_type.as_deref().unwrap_or("—")),
                md_cell(property.unit.as_deref().unwrap_or("—")),
                range_cell(property),
                if property.is_read_only != 0 { "只读" } else { "可写" },
            ));
        }
        out.push('\n');
    }
    out.push_str("## 3. 可用动作\n\n");
    if actions.is_empty() {
        out.push_str("该设备暂未声明动作。\n\n");
    } else {
        out.push_str("| 动作 | 说明 | 参数 |\n|---|---|---|\n");
        for action in actions {
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                md_cell(&action.name),
                md_cell(
                    action
                        .description
                        .as_deref()
                        .or(action.display_name.as_deref())
                        .unwrap_or("—")
                ),
                parameter_text(action.parameters.as_deref()),
            ));
        }
        out.push('\n');
    }
    out.push_str("## 4. 运维要点\n\n");
    out.push_str(&category_guidance(category));
    out.push_str("\n## 5. 关联处置文档\n\n");
    out.push_str(&format!(
        "- 巡检清单、告警处置与常见故障见《{} 巡检与故障处置》。\n",
        display_name(thing)
    ));
    out.push_str("- 修改设定值、下发动作前必须获得审批；执行后回读关键属性确认。\n");
    out
}

fn device_sop(thing: &ThingRow, breadcrumb: &[String], properties: &[PropertyRow]) -> String {
    let category = thing.category.as_deref().unwrap_or("未分类");
    let location = breadcrumb.last().map(String::as_str).unwrap_or("现场");
    let mut out = String::new();
    out.push_str(&format!("# {} 巡检与故障处置\n\n", display_name(thing)));
    out.push_str(&format!(
        "> 适用位置：{} · 分类：{} · 设备 ID：`{}`\n\n",
        location, category, thing.id
    ));
    out.push_str("## 1. 巡检清单\n\n");
    out.push_str("- [ ] 确认设备在线，最近一次数据上报时间正常。\n");
    for property in properties.iter().take(4) {
        out.push_str(&format!(
            "- [ ] 核对「{}」当前值与量程（{}）。\n",
            property.display_name.as_deref().unwrap_or(&property.name),
            range_cell(property)
        ));
    }
    out.push_str("- [ ] 检查现场无异味、异响、积水、松动或明显温升。\n");
    out.push_str("\n## 2. 告警处置\n\n");
    out.push_str(&category_alarm_steps(category));
    out.push_str("\n## 3. 常见故障\n\n");
    out.push_str("| 现象 | 可能原因 | 处置 |\n|---|---|---|\n");
    out.push_str(category_faults(category));
    out.push_str("\n## 4. 升级条件\n\n");
    out.push_str("- 告警持续超过 10 分钟且现场无人确认。\n");
    out.push_str("- 出现焦味、冒烟、明火、积水、人员受困等安全风险。\n");
    out.push_str("- 需要断电、拆机、登高、动火或更换关键部件。\n");
    out.push_str("- 同一区域多台设备同时异常，疑似供电/网络/环境问题。\n");
    out.push_str("\n> 写设备动作必须先获得审批，执行后回读验证；禁止凭单点噪声下结论。\n");
    out
}

fn area_guide(thing: &ThingRow, breadcrumb: &[String], children: &[&ThingRow]) -> String {
    let category = thing.category.as_deref().unwrap_or("区域");
    let location = if breadcrumb.is_empty() {
        display_name(thing).to_string()
    } else {
        breadcrumb.join(" / ")
    };
    let mut out = String::new();
    out.push_str(&format!("# {} 区域说明与巡检路线\n\n", display_name(thing)));
    out.push_str(&format!(
        "> 位置：{} · 区域类型：{} · 节点 ID：`{}`\n\n",
        location, category, thing.id
    ));
    out.push_str("## 1. 区域概况\n\n");
    out.push_str(thing.description.as_deref().unwrap_or("演示工厂区域。"));
    out.push_str("\n\n");
    out.push_str("## 2. 下辖设备\n\n");
    if children.is_empty() {
        out.push_str("该区域暂无直接下辖设备。\n\n");
    } else {
        out.push_str("| 设备 | 类型 | 分类 |\n|---|---|---|\n");
        for child in children {
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                md_cell(display_name(child)),
                md_cell(&child.thing_type),
                md_cell(child.category.as_deref().unwrap_or("—")),
            ));
        }
        out.push('\n');
    }
    out.push_str("## 3. 环境与安全要求\n\n");
    out.push_str("- 保持通道畅通，消防设施前不得堆物；动火作业需审批并配置灭火器材。\n");
    out.push_str("- 配电区域禁止无关人员进入，操作前确认绝缘工具与断电状态。\n");
    out.push_str("- 冷库进入前确认门锁/应急开门装置有效，避免长时间滞留。\n");
    out.push_str("- 发现异味、冒烟、积水、异常温升时先撤人、再上报，不盲目处置。\n\n");
    out.push_str("## 4. 巡检路线\n\n");
    out.push_str("1. 从区域入口进入，先看整体环境与安全通道。\n");
    out.push_str("2. 按设备清单逐台核对在线状态与关键属性。\n");
    out.push_str("3. 检查消防、门禁、照明等公共设施。\n");
    out.push_str("4. 记录异常并标注位置，离开前确认无新增风险。\n\n");
    out.push_str("## 5. 应急\n\n");
    out.push_str("- 火警：触发手动报警、撤离、拨打应急电话，不乘电梯。\n");
    out.push_str("- 触电/设备伤人：先断电再施救，不直接接触伤者。\n");
    out.push_str("- 冷库困人：使用应急开门装置并立即通知中控。\n");
    out
}

fn category_guidance(category: &str) -> String {
    let text = if category.contains("温度记录仪") || category.contains("冷链") {
        "- 冷库温度异常先查门磁、化霜周期与压缩机运行状态。\n- 不得长时间开门；维修时使用应急照明与防冻手套。\n- 温度探头需定期校准，避免贴壁或靠近蒸发器。\n"
    } else if category.contains("电力仪表") {
        "- 电压、电流、功率因数需与分支回路交叉核对；偏差过大先查互感器/接线。\n- 带电作业禁止；验电、挂牌、上锁后再检修。\n- 电能数据用于能耗分析，异常跳变先排查通信与采样周期。\n"
    } else if category.contains("智能水表") {
        "- 压力骤降结合分区流量判断漏水；先关阀再排查管网。\n- 水表井作业注意通风、防滑与有限空间审批。\n- 长期零流量但压力正常，检查表计卡死或通信中断。\n"
    } else if category.contains("照明控制器") {
        "- 照明异常先确认开关、亮度设定与回路空开；夜间作业需临时照明。\n- 更换灯具前断电并冷却，避免高处坠落。\n- 能耗异常升高时核对是否长明灯或亮度被手动拉满。\n"
    } else if category.contains("消防探测器") {
        "- 火警优先：先确认现场、撤离人员并联动消防，不得仅远程消音。\n- 定期自检、清洁探头；故障/低电量及时更换。\n- 烟感误报需结合温感、视频与现场气味交叉验证。\n"
    } else if category.contains("门禁控制器") {
        "- 门磁/锁状态不一致先查供电与网络；应急通道不得上锁。\n- 频繁失败刷卡按安全事件处理，保留记录并上报。\n- 断电前确认应急开门方式。\n"
    } else if category.contains("网络摄像头") {
        "- 画面异常先查供电、网络与存储；镜头清洁避免遮挡。\n- 视频涉及隐私，调阅/导出需授权并留痕。\n- 存储临近满时及时扩容或调整保留策略。\n"
    } else if category.contains("协作机器人") {
        "- 任何进入工作半径的操作先急停/断电；急停后需人工复位。\n- 回零、改速度、移动坐标前确认周边无人。\n- 碰撞/异响/过热立即停机并检查机械与驱动器。\n"
    } else if category.contains("环境传感器") {
        "- 温湿度异常先排除安装位置、阳光直射、空调出风与电池电量。\n- 传感器需定期校准；与同区域设备交叉核对。\n- 高湿环境注意凝露与探头污染。\n"
    } else if category.contains("边缘网关") {
        "- 网关 CPU/内存/连接数异常先查下挂设备与网络风暴。\n- 重启前确认离线缓存与采集任务；避免频繁重启。\n- 固件升级需审批并保留回滚方案。\n"
    } else if category.contains("安防主机") {
        "- 布防/撤防按班次执行，交接班确认防区状态。\n- 入侵报警先核实画面与现场，再决定出警。\n- 备用电池定期测试，避免断电失效。\n"
    } else {
        "- 按设备铭牌与现场规程操作；异常先停机、断电、上报。\n- 定期清洁、紧固、校准；保留维护记录。\n- 写操作需审批，执行后回读关键属性。\n"
    };
    text.to_string()
}

fn category_alarm_steps(category: &str) -> String {
    if category.contains("温度记录仪") || category.contains("冷链") {
        "1. 确认门是否关闭、是否处于化霜/除霜周期。\n2. 查看压缩机运行状态与回风温度。\n3. 温度持续上升或接近药品安全上限时，转移物品并上报。\n".to_string()
    } else if category.contains("消防探测器") {
        "1. 立即确认现场是否有烟/火/焦味，联动视频复核。\n2. 真实火警按消防预案撤离并报警，不得远程消音代替处置。\n3. 误报需记录原因（粉尘、蒸汽、烹饪、施工）并安排清洁/复位。\n".to_string()
    } else if category.contains("电力仪表") {
        "1. 核对电压/电流/功率因数与分支回路，判断是总表还是分支异常。\n2. 过载先降负荷；电压异常联系配电值守。\n3. 禁止带电排查，必要时断电挂牌。\n".to_string()
    } else if category.contains("智能水表") {
        "1. 压力低结合流量判断漏水/爆管，先关阀。\n2. 检查水泵、阀门与管网；积水区域先断电。\n3. 恢复后缓慢升压并复查。\n".to_string()
    } else if category.contains("门禁控制器") || category.contains("安防主机") {
        "1. 核实画面、门磁与现场人员。\n2. 疑似入侵立即上报并保留记录，不单独处置。\n3. 设备故障先确认应急通道可用。\n"
            .to_string()
    } else {
        "1. 结合相邻设备与历史事件判断是否为单点噪声。\n2. 持续越限或伴随安全风险时升级人工。\n3. 需要写设备动作时先审批，执行后回读验证。\n".to_string()
    }
}

fn category_faults(category: &str) -> &'static str {
    if category.contains("温度记录仪") || category.contains("冷链") {
        "| 温度持续偏高 | 门未关、化霜异常、压缩机故障、探头漂移 | 关门/查化霜/查压缩机/校准探头 |\n| 温度骤降 | 探头结霜、制冷过强、设定值被改 | 除霜、核对设定值、校准 |\n| 数据中断 | 网关/电池/通信故障 | 查网关与电池，恢复后补采 |\n"
    } else if category.contains("消防探测器") {
        "| 误报 | 粉尘、蒸汽、施工、探头污染 | 清洁、复位、记录并观察 |\n| 故障灯 | 探头失效、电池低、回路断线 | 更换/查线/上报消防维保 |\n| 无法自检 | 供电或通信异常 | 查供电与总线，必要时更换 |\n"
    } else if category.contains("电力仪表") {
        "| 电压异常 | 市电波动、缺相、互感器故障 | 联系配电值守，禁止带电操作 |\n| 电流不平衡 | 负荷分配不均、缺相 | 调整负荷/查线路 |\n| 通信不上 | 485 接线、地址、波特率 | 核对接线与参数 |\n"
    } else if category.contains("照明控制器") {
        "| 灯不亮 | 空开跳闸、驱动/灯具故障、回路断线 | 断电后查空开与灯具 |\n| 无法调光 | 信号线/协议异常 | 查总线与地址 |\n| 能耗偏高 | 长明灯、亮度拉满、回路混接 | 核对策略与回路 |\n"
    } else if category.contains("智能水表") {
        "| 流量为零 | 阀门关闭、表计卡死、通信中断 | 查阀/表/通信 |\n| 压力低 | 漏水、泵故障、管网堵塞 | 关阀、查泵、查管网 |\n| 读数跳变 | 气泡、探头污染、干扰 | 排气、清洁、复查 |\n"
    } else if category.contains("门禁控制器") {
        "| 无法开门 | 断电、锁体故障、权限错误 | 查供电/锁体/权限 |\n| 门磁异常 | 磁簧松动、线路断 | 调整/更换 |\n| 频繁失败 | 读卡器故障或安全事件 | 查设备并上报 |\n"
    } else if category.contains("网络摄像头") {
        "| 无画面 | 断电、网络中断、IP 冲突 | 查供电/网络/IP |\n| 画面模糊 | 镜头脏污、对焦漂移 | 清洁/调焦 |\n| 存储满 | 保留策略/磁盘故障 | 扩容/调整策略 |\n"
    } else if category.contains("协作机器人") {
        "| 急停无法复位 | 安全回路未恢复、急停未释放 | 排查安全回路后复位 |\n| 异响/抖动 | 减速机/导轨/负载异常 | 停机检查机械 |\n| 定位偏差 | 零点丢失、编码器故障 | 重新回零/校准 |\n"
    } else if category.contains("环境传感器") {
        "| 读数漂移 | 探头老化、污染、安装位置 | 清洁/校准/调整位置 |\n| 数据不动 | 通信中断、电池低 | 查网关/更换电池 |\n| 湿度异常 | 凝露、蒸汽直吹 | 调整安装、除湿 |\n"
    } else if category.contains("边缘网关") {
        "| 下挂设备离线 | 网关负载高、网络故障、供电 | 查负载/网络/电源 |\n| CPU 高 | 采集过密、规则过多、风暴 | 降频/限流/排查 |\n| 数据丢失 | 缓存/磁盘/断网 | 查磁盘与补传 |\n"
    } else if category.contains("安防主机") {
        "| 布防失败 | 防区开路、电池低、通信故障 | 查防区/电池/网络 |\n| 误报 | 门磁/红外被人为触发 | 核实现场并记录 |\n| 断电后失效 | 备用电池老化 | 更换电池 |\n"
    } else {
        "| 离线 | 供电/网络/驱动故障 | 查供电与网络，重启后观察 |\n| 数据异常 | 探头/通信/干扰 | 交叉核对，校准或更换 |\n| 动作失败 | 设备忙、联锁、权限 | 查状态与审批，禁止强推 |\n"
    }
}

#[allow(clippy::too_many_arguments)]
async fn insert_doc(
    pool: &SqlitePool,
    thing: &ThingRow,
    suffix: &str,
    name: &str,
    description: &str,
    content: &str,
    tags: &[String],
    kind: &str,
) -> Result<(), sqlx::Error> {
    let id = format!("doc-{}-{suffix}", thing.id);
    let now = chrono::Utc::now().to_rfc3339();
    let tags = serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string());
    let metadata = serde_json::json!({
        "v": 1,
        "generated": true,
        "kind": kind,
        "thingType": thing.thing_type,
    })
    .to_string();
    sqlx::query(
        "INSERT OR IGNORE INTO resources \
         (id, workspace_id, thing_id, resource_type, name, description, content, file_path, tags, metadata, created_at, updated_at) \
         VALUES (?, ?, ?, 'document', ?, ?, ?, '', ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(WORKSPACE_ID)
    .bind(&thing.id)
    .bind(name)
    .bind(description)
    .bind(content)
    .bind(&tags)
    .bind(&metadata)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}
