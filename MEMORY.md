# Project memory

## 多板排料需求与工具链

用户最新明确纠正利用率口径并通过选项确认：单板U=原始零件总面积/(占用长度*板材固定宽度)，占用长度从x=0到所有原始零件最右侧边界(不减minx)；多板overall=sum原始零件面积/sum各板占用长度*固定板宽。之前整板面积率只作辅助full_board_utilization，不是优化主目标。取消加权，开新板priority降序同级输入顺序，库存/固定边界硬限制，同利用率时每开一张惩罚1优先板少。保留内核，外层持续搜索到deadline并对已有best逐板热启动压缩(不能FitListener fit即停)。live_svg单SVG多板拼图，viewer长图最大640px居中。RustCargo绝对路径C:/Users/Huawei/.cargo/bin，PATH可能含换行。
