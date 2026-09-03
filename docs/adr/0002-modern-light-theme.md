# 浅色定制主题（kill-ai-slop 约束）

界面从 egui 默认样式改为定制浅色主题：单一 accent（Steam 蓝）+ 中性灰底 + hairline 卡片边框 + 小圆角（约 4px），主操作按钮 accent 填充、次操作描边按钮，标题层级靠字号/粗细拉开。窗口改为可缩放并设最小尺寸（原 560×470 固定）。

> 已被 ADR-0003 取代：配色与组件形态（accent bar、独立按钮区、纯文字状态）现对齐 Python 版，见 `0003-python-parity-restyle.md`。本 ADR 保留的约束：可缩放窗口、kill-ai-slop 红线。

硬性约束：不引入渐变、毛玻璃、发光状态点、emoji、圆角嵌套、卡片套卡片、AI 文案腔（kill-ai-slop taxonomy 红线）。状态用 4px 平色圆点+词（运行绿/未运行灰），错误保留红色、成功/进行中改中性色（收敛默认语义三连）。
