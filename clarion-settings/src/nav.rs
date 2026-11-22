//! Sidebar catalogue and page identity. Metadata stays independent of the
//! renderer so search, deep links and tests share one source of truth.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pane {
    Overview,
    Appearance,
    Dock,
    MenuBar,
    Accessibility,
    WindowEffects,
    Layout,
    MissionControl,
    Displays,
    Power,
    Keyboard,
    Trackpad,
    System,
}

#[derive(Clone, Copy)]
pub struct NavItem {
    pub pane: Pane,
    pub title: &'static str,
    #[allow(dead_code)]
    pub subtitle: &'static str,
    pub aliases: &'static [&'static str],
    pub group: usize,
    pub icon: &'static str,
    #[allow(dead_code)]
    pub implemented: bool,
}

pub const GROUP_TITLES: [&str; 5] = ["", "桌面", "窗口", "设备", ""];

pub const NAV: [NavItem; 13] = [
    NavItem {
        pane: Pane::Appearance,
        title: "外观",
        subtitle: "主题、强调色与半透明材质",
        aliases: &["appearance", "theme", "外观", "主题", "强调色", "透明度", "磨砂"],
        group: 1,
        icon: crate::icons::APPEARANCE,
        implemented: true,
    },
    NavItem {
        pane: Pane::Dock,
        title: "程序坞",
        subtitle: "大小、放大、位置与行为",
        aliases: &["dock", "程序坞", "停靠栏", "放大", "自动隐藏", "启动台"],
        group: 1,
        icon: crate::icons::DOCK,
        implemented: true,
    },
    NavItem {
        pane: Pane::MenuBar,
        title: "菜单栏",
        subtitle: "高度、背景与时钟",
        aliases: &["menu bar", "menubar", "菜单栏", "时钟", "日期", "秒"],
        group: 1,
        icon: crate::icons::MENU_BAR,
        implemented: true,
    },
    NavItem {
        pane: Pane::Accessibility,
        title: "辅助功能",
        subtitle: "减少动态效果",
        aliases: &["accessibility", "辅助功能", "减少动态", "动效"],
        group: 1,
        icon: crate::icons::ACCESSIBILITY,
        implemented: true,
    },
    NavItem {
        pane: Pane::WindowEffects,
        title: "窗口与动效",
        subtitle: "边框、阴影、模糊与过渡",
        aliases: &["effects", "animation", "动效", "动画", "边框", "阴影", "模糊"],
        group: 2,
        icon: crate::icons::WINDOW_EFFECTS,
        implemented: true,
    },
    NavItem {
        pane: Pane::Layout,
        title: "窗口布局",
        subtitle: "间距、平铺、画布与触发角",
        aliases: &[
            "layout",
            "tiling",
            "hot-corners",
            "平铺",
            "间距",
            "画布",
            "触发角",
        ],
        group: 2,
        icon: crate::icons::LAYOUT,
        implemented: true,
    },
    NavItem {
        pane: Pane::MissionControl,
        title: "调度中心",
        subtitle: "工作区与窗口管理",
        aliases: &["windows", "workspaces", "mission control", "工作区"],
        group: 2,
        icon: crate::icons::MISSION_CONTROL,
        implemented: true,
    },
    NavItem {
        pane: Pane::Displays,
        title: "显示器",
        subtitle: "分辨率、缩放与排列",
        aliases: &["displays", "outputs", "显示器", "分辨率", "缩放", "刷新率"],
        group: 3,
        icon: crate::icons::DISPLAYS,
        implemented: true,
    },
    NavItem {
        pane: Pane::Power,
        title: "电源与锁屏",
        subtitle: "空闲、熄屏与锁定",
        aliases: &[
            "power",
            "idle",
            "lock",
            "电源",
            "锁屏",
            "休眠",
            "熄屏",
            "空闲",
        ],
        group: 3,
        icon: crate::icons::POWER,
        implemented: true,
    },
    NavItem {
        pane: Pane::Keyboard,
        title: "键盘",
        subtitle: "重复速度与快捷键",
        aliases: &["keyboard", "input", "键盘", "输入", "快捷键"],
        group: 3,
        icon: crate::icons::KEYBOARD,
        implemented: true,
    },
    NavItem {
        pane: Pane::Trackpad,
        title: "输入设备",
        subtitle: "触控板、鼠标与指点杆",
        aliases: &[
            "trackpad",
            "touchpad",
            "mouse",
            "pointing stick",
            "输入设备",
            "触控板",
            "鼠标",
            "指点杆",
            "手势",
            "滚动",
        ],
        group: 3,
        icon: crate::icons::TRACKPAD,
        implemented: true,
    },
    NavItem {
        pane: Pane::System,
        title: "系统",
        subtitle: "截图、Xwayland 与集成",
        aliases: &[
            "system",
            "integration",
            "screenshot",
            "xwayland",
            "系统",
            "截图",
            "集成",
        ],
        group: 3,
        icon: crate::icons::SYSTEM,
        implemented: true,
    },
    NavItem {
        pane: Pane::Overview,
        title: "关于本机",
        subtitle: "设备信息、系统与配置文件",
        aliases: &["overview", "about", "dashboard", "关于", "关于本机", "本机", "总览", "系统信息"],
        group: 4,
        icon: crate::icons::OVERVIEW,
        implemented: true,
    },
];

pub fn item(pane: Pane) -> &'static NavItem {
    NAV.iter().find(|item| item.pane == pane).unwrap_or(&NAV[0])
}

pub fn matching(query: &str) -> Vec<&'static NavItem> {
    let query = query.trim().to_lowercase();
    NAV.iter()
        .filter(|item| {
            query.split_whitespace().all(|term| {
                item.title.to_lowercase().contains(term)
                    || item
                        .aliases
                        .iter()
                        .any(|alias| alias.to_lowercase().contains(term))
            })
        })
        .collect()
}

pub fn from_arg(value: &str) -> Option<Pane> {
    let value = value.trim().to_ascii_lowercase();
    NAV.iter()
        .find(|item| {
            item.title.to_lowercase() == value
                || item.aliases.iter().any(|alias| *alias == value)
                || match item.pane {
                    Pane::Overview => matches!(value.as_str(), "overview" | "about"),
                    Pane::Appearance => value == "appearance",
                    Pane::Dock => value == "dock",
                    Pane::Power => matches!(value.as_str(), "power" | "idle" | "lock"),
                    Pane::MenuBar => matches!(value.as_str(), "menu-bar" | "menubar"),
                    Pane::Accessibility => value == "accessibility",
                    Pane::WindowEffects => matches!(value.as_str(), "effects" | "window-effects"),
                    Pane::Layout => value == "layout",
                    Pane::MissionControl => matches!(value.as_str(), "mission-control" | "windows"),
                    Pane::Displays => matches!(value.as_str(), "displays" | "outputs"),
                    Pane::Keyboard => matches!(value.as_str(), "keyboard" | "input"),
                    Pane::Trackpad => value == "trackpad",
                    Pane::System => matches!(value.as_str(), "system" | "screenshot" | "xwayland"),
                }
        })
        .map(|item| item.pane)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pane_is_unique_and_search_finds_the_desktop_pages() {
        for item in NAV {
            assert_eq!(NAV.iter().filter(|other| other.pane == item.pane).count(), 1);
            assert_eq!(from_arg(item.title), Some(item.pane));
        }
        for (query, pane) in [
            ("程序坞", Pane::Dock),
            ("强调色", Pane::Appearance),
            ("菜单栏", Pane::MenuBar),
            ("减少动态", Pane::Accessibility),
        ] {
            assert_eq!(matching(query).iter().map(|item| item.pane).collect::<Vec<_>>(), [pane]);
        }
    }
}
