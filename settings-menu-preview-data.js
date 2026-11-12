window.settingsPreviewPages = [
  {
    id: "interface",
    label: "界面与字体",
    icon: "i-palette",
    intro: "主题、语言、界面密度和文字显示。",
    sections: [
      { title: "主题与语言", target: "appearance-page" },
      { title: "字体与编辑", target: "font-page" },
    ],
    search: "主题 颜色 窗口缩放 语言 字体 编辑器 终端 连字 换行",
  },
  {
    id: "sessions",
    label: "会话与输入",
    icon: "i-message",
    intro: "会话列表、回复呈现和输入效率。",
    search: "侧边栏 完成会话 排序 回复 执行过程 快捷键 快捷消息 标签页 模式选择 智能界面",
    sections: [
      {
        title: "会话列表与标签",
        items: [
          { label: "显示已完成会话", detail: "已完成的会话保留在侧边栏。", type: "switch", checked: true },
          { label: "会话排序", type: "select", value: "按更新时间", options: ["按更新时间", "按创建时间"] },
          { label: "侧边栏区域顺序", type: "select", value: "文件夹在上", options: ["文件夹在上", "聊天在上"] },
          { label: "会话标签页上限", detail: "设为 0 表示不限制。", type: "number", value: "12", suffix: "个" },
        ],
      },
      {
        title: "回复与执行过程",
        items: [
          { label: "回答风格", type: "select", value: "标准", options: ["简洁", "标准", "详尽"] },
          { label: "过程默认显示", type: "select", value: "重点摘要", options: ["重点摘要", "完整过程", "极简结果"] },
          { label: "完成后收起整轮过程", type: "switch", checked: true },
          { label: "失败时自动展开", detail: "错误、超时和拒绝默认展开。", type: "switch", checked: true },
          { label: "智能界面", detail: "允许界面根据当前任务展示相关操作入口。", type: "switch", checked: true },
        ],
      },
      {
        title: "输入效率",
        items: [
          { label: "常用操作快捷键", detail: "可为打开设置、新建会话、切换终端等操作设置按键。", type: "button", button: "查看快捷键" },
          { label: "快捷消息", detail: "集中管理可插入输入框的常用文本。", type: "button", button: "管理快捷消息" },
          { label: "新会话模式卡片", detail: "代码开发 / 日常办公入口。", type: "switch", checked: true },
        ],
      },
    ],
  },
  {
    id: "agents",
    label: "智能体与协同",
    icon: "i-bot",
    intro: "智能体配置、默认行为和多智能体协同。",
    search: "智能体 安装 启用 默认 顺序 委派 深度 子智能体 缓存",
    sections: [
      {
        title: "已安装智能体",
        items: [
          { label: "启用状态", detail: "控制哪些智能体可以在新会话中使用。", type: "status", value: "8 个已启用" },
          { label: "智能体顺序", detail: "调整新建会话时的选择顺序。", type: "button", button: "调整顺序" },
          { label: "安装与运行配置", detail: "查看安装版本、启动参数和环境变量。", type: "button", button: "管理智能体" },
        ],
      },
      {
        title: "会话默认行为",
        items: [
          { label: "默认智能体", detail: "新建会话时优先选择的智能体。", type: "select", value: "星河", options: ["星河", "远山", "云舟", "流光"] },
          { label: "默认执行模式", detail: "由智能体支持的配置项决定可选范围。", type: "select", value: "跟随智能体", options: ["跟随智能体", "标准", "规划"] },
          { label: "失败后的处理", detail: "启动失败时保留草稿并显示可重试状态。", type: "status", value: "保留草稿" },
        ],
      },
      {
        title: "多智能体协同",
        items: [
          { label: "允许委派子任务", detail: "由当前智能体将子任务交给其他智能体。", type: "switch", checked: true },
          { label: "最大委派深度", type: "number", value: "3", suffix: "层", min: "1", max: "8" },
          { label: "已完成结果缓存", detail: "设为 0 表示不限制；会话结束后清除。", type: "number", value: "512", suffix: "MB", min: "0" },
          { label: "子智能体默认配置", detail: "为委派启动的子智能体设置模式与配置覆盖。", type: "button", button: "配置默认项" },
        ],
      },
    ],
  },
  {
    id: "capabilities",
    label: "技能与记忆",
    icon: "i-package",
    intro: "管理可用技能、个人记忆和自动维护。",
    search: "技能 办公 联网 电脑操作 用户记忆 学习 写入 继承 维护 诊断",
    sections: [
      {
        title: "技能与工具",
        items: [
          { label: "技能包", detail: "办公、联网和电脑操作技能。能力市场入口仍在首页。", type: "button", button: "管理技能包" },
          { label: "已安装技能自动更新", detail: "插件包仍需手动更新。", type: "switch", checked: true },
        ],
      },
      {
        title: "用户记忆",
        items: [
          { label: "启用用户记忆", detail: "控制会话记忆的学习与检索。", type: "switch", checked: true },
          { label: "允许智能体写入记忆", type: "switch", checked: true },
          { label: "向子智能体继承记忆", type: "switch", checked: false },
          { label: "记忆条目、治理与维护", detail: "查看记忆来源、效果和诊断状态。", type: "button", button: "管理记忆" },
        ],
      },
    ],
  },
  {
    id: "connections",
    label: "连接与通知",
    icon: "i-send",
    intro: "网络代理、内置浏览器、消息渠道和提醒。",
    search: "代理 网络 http socks 浏览器 WebView Chromix Cookie 登录 消息渠道 机器人 通知",
    sections: [
      {
        title: "网络",
        items: [
          { label: "启用网络代理", type: "switch", checked: false },
          { label: "代理地址", detail: "支持 HTTP(S) / SOCKS5。", type: "text", value: "http://127.0.0.1:7890", placeholder: "代理地址" },
        ],
      },
      {
        title: "内置浏览器",
        items: [
          { label: "显示内置浏览器", detail: "供用户与智能体共用。", type: "switch", checked: true },
          { label: "浏览器内核", type: "select", value: "轻量浏览器 · WebView", options: ["轻量浏览器 · WebView", "完整浏览器 · Chromix"] },
          { label: "保留网站登录状态", detail: "用户与智能体共用网站数据。", type: "switch", checked: true },
        ],
      },
      {
        title: "消息渠道与提醒",
        items: [
          { label: "消息渠道", detail: "机器人接收事件通知和查询编码活动。", type: "button", button: "管理消息渠道" },
          { label: "任务完成通知", detail: "离开原助理界面时通知任务完成。", type: "switch", checked: true },
        ],
      },
    ],
  },
  {
    id: "maintenance",
    label: "系统维护",
    icon: "i-wrench",
    intro: "启动、电源、更新、运行环境和数据维护。",
    search: "开机 自启动 保持唤醒 更新 静默 技能 渲染加速 Windows 运行环境 修复 诊断 日志 备份 恢复 语言",
    sections: [
      {
        title: "启动与电源",
        items: [
          { label: "开机自启动", detail: "在后台启动，可从系统托盘唤回。", type: "switch", checked: false },
          { label: "运行期间保持唤醒", detail: "退出应用后释放防休眠状态。", type: "switch", checked: false },
        ],
      },
      {
        title: "应用与技能更新",
        items: [
          { label: "自动检查应用更新", type: "switch", checked: true },
          { label: "静默自动更新", detail: "空闲且没有运行任务时安装并重启。", type: "switch", checked: true },
          { label: "能力市场技能自动更新", detail: "普通技能和专家包自动更新。", type: "switch", checked: true },
          { label: "更新通道", type: "select", value: "稳定版", options: ["稳定版", "测试版"] },
        ],
      },
      {
        title: "运行环境与渲染",
        items: [
          { label: "环境组件", detail: "运行时状态：待连接应用读取。", type: "button", button: "检查环境" },
          { label: "修复异常组件", detail: "仅修复检测异常的组件。", type: "button", button: "检查并修复" },
          { label: "禁用硬件加速", detail: "仅 Windows 桌面端生效；需重启。", type: "switch", checked: false },
        ],
      },
      {
        title: "日志与数据",
        items: [
          { label: "问题反馈", detail: "检查将要提交的诊断记录。", type: "button", button: "查看反馈设置" },
          { label: "应用数据备份", detail: "导出或恢复设置与本地数据。", type: "button", button: "备份与恢复" },
        ],
      },
    ],
  },
  {
    id: "insights",
    label: "用量与性能",
    icon: "i-usage",
    intro: "查看使用情况与本机资源状态。",
    search: "用量 token 会话 模型 CPU 内存 进程 性能 监控",
    sections: [
      {
        title: "模型用量",
        items: [
          { label: "统计周期", type: "select", value: "最近 7 天", options: ["最近 7 天", "最近 30 天", "全部"] },
          { label: "用量明细", detail: "运行应用后读取实际会话和模型用量。", type: "button", button: "刷新用量" },
        ],
      },
      {
        title: "资源监控",
        items: [
          { label: "CPU / 内存 / 进程", detail: "实时数据由运行中的应用提供；此独立预览不伪造数值。", type: "status", value: "等待实时数据" },
          { label: "性能详情", detail: "查看进程分组与内存状态。", type: "button", button: "查看性能监控" },
        ],
      },
    ],
  },
];
