# sparrow 🪶 
[![DOI](https://zenodo.org/badge/DOI/10.48550/arXiv.2509.13329.svg)](https://doi.org/10.48550/arXiv.2509.13329)
[![CI](https://github.com/JeroenGar/sparrow/actions/workflows/rust_ci.yml/badge.svg?branch=main)](https://github.com/JeroenGar/sparrow/actions/workflows/rust_ci.yml) 
[![Unique clones](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/JeroenGar/sparrow/traffic/clones.json)](https://github.com/JeroenGar/sparrow/tree/traffic)
[![Unique views](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/JeroenGar/sparrow/traffic/views.json)](https://github.com/JeroenGar/sparrow/tree/traffic)

<p>
    <img src="data/sparrow-icon.svg" align="right" alt="logo" height=80>

> Sparrows are master weavers, crafting nests with intricate patterns. They utilize geometry and symmetry to ensure structural integrity and stability. By incorporating precise angles,  lengths, and weaving patterns, these birds achieve a balance between strength and efficiency.
[(read more)](https://www.mathnasium.com/math-centers/happyvalley/news/mathematical-marvels-bird-nest-construction-hv#)

</p>

### The state-of-the-art nesting heuristic for 2D irregular strip packing
`sparrow` can be used to solve 2D irregular strip packing problems, also commonly referred to as nesting problems.
This optimization algorithm builds on [`jagua-rs`](https://github.com/JeroenGar/jagua-rs): _a collision detection engine for 2D irregular cutting & packing problems_.

This repository accompanies the paper: ["_An open-source heuristic to reboot 2D nesting research_"](https://doi.org/10.48550/arXiv.2509.13329).

> [!TIP]
> **[Try 2D nesting with sparrow in your browser](https://sparrowstudio.app/)**
>
> Import SVG, DXF or instance JSON, configure the nesting job, watch the search live and download the result. It runs locally on your device, with no installation.
>
> [<img width="500" alt="image" src="https://github.com/user-attachments/assets/4d84bb67-ff98-4310-82de-5350baa02427" />](https://sparrowstudio.app/)

## `sparrow` in action

![The `sparrow` TUI dashboard running alongside the live solution viewer](data/demo.gif)

## Nested by `sparrow`
<p align="center">
    <img src="data/records/final_best_trousers.svg" height=200/>
    <img src="data/records/final_best_mao.svg" height=200/>
</p>
<p align="center">
    <img src="data/records/final_best_swim.svg" height=270/>
    <img src="data/records/final_best_marques.svg" height=270/>
    <img src="data/records/final_best_dagli.svg" height=270/>
</p>
<p align="center">
    <img src="data/records/final_best_albano.svg" height=200/>
    <img src="data/records/final_best_shirts.svg" height=200/>
</p>
<p align="center">
    <img src="data/records/final_best_gardeyn1.svg" height=220/>
    <img src="data/records/final_best_gardeyn8.svg" height=220/>
</p>

## Requirements
- [Rust](https://www.rust-lang.org/tools/install) ≥ 1.90

## Usage

**General usage:**
```bash
cargo run --release  -- \
    -i [path to input JSON, or a solution JSON] \
    -t [timelimit in seconds (default is 600s)]
```
The optimization process contains two distinct phases: exploration & compression.
By default 80% of the timelimit is spent exploring and 20% is spent compressing.
Pressing 'Ctrl + C' immediately moves the algorithm to the next phase, or terminates it.

**All CLI options:**
```bash
-i, --input <INPUT>              Path to the input JSON file, or a solution JSON file for warm starting
-t, --global-time <GLOBAL_TIME>  Set a global time limit (in seconds)
-e, --exploration <EXPLORATION>  Set the exploration phase time limit (in seconds)
-c, --compression <COMPRESSION>  Set the compression phase time limit (in seconds)
-x, --early-termination          Enable early termination of the optimization process
-s, --rng-seed <RNG_SEED>        Fixed seed for the random number generator
--workers <WORKERS>           Number of worker threads used by the separator
-h, --help                       Print help
```

**Concrete example**:
```bash
cargo run --release -- \
    -i data/input/swim.json
```

## 多板固定尺寸排料（独立入口）

原有 `sparrow` CLI、TUI 和优化内核保持不变。新增入口：

```bash
cargo run --release --bin sparrow-multiboard -- -i data/input/multiboard_example.json -t 60 -s 42 --workers 3
```

`-t/--global-time` 为所有板材和候选共享的全局秒数预算，默认 600；
`-s/--rng-seed` 指定随机种子，`--workers` 指定正整数工作线程数。
同一随机种子不保证按墙钟时间停止时的结果逐位相同。

输入包含 `name`、可省略且默认零的 `min_item_separation`、`boards` 和 `items`。
`boards` 每项包含唯一字符串 `id`、有限正数 `length` / `width`、有限库存整数
`quantity` 和有符号整数 `priority`；数字越大，利用率权重越高，不是严格使用顺序约束。
将所有输入板型（含零库存）的不同 `priority` 从小到大排名为 1..k，排名即
`utilization_weight`；同优先级权重相同。负数、零及 i64 极值均按排名处理，
不直接作为权重，也不对 priority 做加减运算。首轮同级按输入顺序检查。
板宽对应原求解器的 `strip_height`，板长对应 `strip_width`。
`items` 完全沿用 jagua 的零件格式（唯一数字 `id`、非负整数 `demand`、
`orientation` 和 `shape`），示例包含非连续 ID 和非原点零件。

外层在全局预算内持续多轮重新构建完整方案，原优化内核不变。首轮按零件原始面积
从大到小逐个增加候选，后续轮打乱零件顺序（有多种零件时保证首选改变），并继续推进随机数状态。
每轮重新初始化需求与库存；首轮和后续奇数轮按优先级降序、同级输入顺序检查板型，
偶数轮随机打乱板型顺序（有多种板型且顺序未变时轮转一次），探索板型用量和零件跨板重新分配。
优先级顺序只是搜索偏好，库存始终是不可超出的硬边界。评分始终使用固定规范顺序，
不会因本轮打乱板序而改变。
首轮可使用剩余预算的一半，不设两秒上限，为大任务留出完成整轮的时间。
后续偶数轮采用至多两秒的短轮，奇数轮的上限依次增长为 4、8、16 秒等（最高 2048 秒）；
所有轮次均不超过剩余预算的一半，以保留继续搜索的机会。候选仍公平分配轮内时间片。
轮次超时只结束本轮，不结束全局搜索。
即使本轮已全部排完、库存耗尽或没有找到布局，也继续重新搜索直到全局截止或中断，
不通过休眠消耗预算。无需求、无板型或全部库存为零时无需搜索，可直接结束。
面积只用于筛选，绝不作为几何可行性判断。候选未找到合格布局时，在预算允许下
最多尝试两次；找到板长内的可行解后提前停止该候选，避免无益的继续压缩。
每次优化阶段使用至多一秒时间片，探索/压缩按 80%/20% 分配。
原内核返回错误或可捕获的 panic 时，该候选计为失败并继续有限重试；
panic 信息仍可能出现在 stderr，进程中止型故障不保证可恢复。
只接受求解条带长度不超过板长、恢复真实板长后无碰撞且候选需求全部满足的布局，
不通过放宽越界容差接受结果。临时空候选不会丢掉当前板已接受的布局；末轮部分方案
也参与全局比较。所有轮次方案保存在内存，搜索结束后输出最佳方案的独立 SVG 和摘要；
启用下述 `live_svg` 功能时，搜索期间另行更新实时预览快照。
全局比较依次为：已排件数更多、已排原始面积更大；只有排出内容（各零件数量）相同时，
再最大化总体加权利用率：

`weighted_utilization = sum(w * 板上原始零件面积) / sum(w * 已用整板面积)`。

每张实际使用的整板分别计入，未使用库存不计入分母。这是加权面积之比，
不是各板利用率的算术平均。提高高权重板上的利用率对目标影响更大，
但不保证总是用更多高优先级板。无已用板时两种利用率均为 0。
加权值相同时依次选择更小总板面积（同内容时等价于更高普通总体利用率）、更少总板数，
最后才按固定优先级降序的逆序逐项选择更少板型用量，同级输入靠后的板型先比较。
件数和面积相同但内容不同、或全部指标相同时保留之前最佳结果；绝不为提高利用率丢弃零件。
每次完成一张板后从本轮板序的开头检查。某板型本轮有限尝试未找到布局时才检查后续板型；
启发式失败不是不可行性证明。

**时间限制不是硬实时保证**：原内核初始构造和部分内部操作不支持抢占，导入、构造、
验证及输出可能使实际耗时超过预算。Ctrl+C 设置外层共享停止标志，内核在下次检查
终止条件时停止，随后导出已有结果；不会立即强制中断初始构造。

输出位于 `output/job_{安全化任务名}/summary.json`，以及每个板实例独立的
`board_000001.svg` 等文件；空任务名使用 `output/multiboard`。
任务名仅保留 ASCII 字母、数字、下划线和连字符，其余替换为下划线，限制为 80 字符，
不会按任务名拼接任意路径。相同安全化名称的再次运行会覆盖同名结果；可能保留上一次
多出的 SVG，当前运行实际结果以 `summary.json` 中列出的 SVG 为准。

摘要包含唯一板实例 ID、原板类型和优先级、真实长宽、原始零件面积与整板面积之比、
原始零件 ID 和应用于原始形状的完整变换、库存使用量、未排需求、随机种子、尝试计数及停止原因。
摘要还包含 `overall_utilization = 总原始零件面积 / 已用整板总面积`、上述
`weighted_utilization`，并在每个板实例和库存项中输出整数 `utilization_weight`。
摘要新增 `search_rounds`（已启动轮数）、`best_round`（最佳方案来源轮，从 1 起；
没有改善初始空方案时为 0）和 `all_items_placed`（最佳方案是否满足全部需求）。
`elapsed_seconds` 包含总搜索及 SVG 导出耗时。SVG 显示完整固定尺寸板边界，不裁剪成最短条带。
有搜索需求的任务停止原因仅为 `time_budget_exhausted` 或 `interrupted`，全部排完并非停止原因。
无需求时为 `all_items_placed`，无板型或无库存时为 `inventory_exhausted`。
剩余需求只表示本次预算和搜索策略未找到全部布局，不表示一定不可排。

多板实时预览沿用原单条带查看器，无需 HTTP 服务。运行：

```bash
cargo run --release --features live_svg --bin sparrow-multiboard -- -i data/input/multiboard_example.json -t60
```

在浏览器中打开 [live_viewer.html](data/live/live_viewer.html)。它仍每 100 毫秒读取
`data/live/.live_solution.svg`，现在这张图片会将当前全局最佳方案的所有板实例自上而下拼接，
显示板实例编号、板型、尺寸、优先级、利用率及权重，以及全局已排/需求数、
普通总体利用率、总体加权利用率、最佳轮次和状态。
各板按相同显示宽度、保留纵横比绘制，不表示不同板之间的真实比例；长图在查看器中按页面宽度显示，
可纵向滚动查看所有板，也可直接打开这张 SVG 并放大查看。开局清除上次预览，只有全局最佳方案改善时更新，结束时标记
`final` 及停止原因；未排完的最佳方案同样可预览。最终独立 SVG 和摘要输出规则不变。
原单条带入口与查看器保持兼容；两个进程不要同时写同一个实时预览文件。
库调用的 `MultiBoardOptions.live_svg_path` 默认为 `None`，可指定独立文件；仅启用
`live_svg` feature 时才写入该路径，未启用时完全忽略。

## Live visualization

`sparrow` can show the search in a terminal dashboard alongside a live browser view of the current solution.
The dashboard reports the current phase, progress, throughput, and logs. It continuously updates the SVG used by
[live_viewer.html](data/live/live_viewer.html):

```bash
cargo run --release --features=tui -- \
    -i data/input/swim.json
```

For the browser viewer without the dashboard, open
[live_viewer.html](data/live/live_viewer.html) and enable the `live_svg` feature:

```bash
cargo run --release --features=live_svg -- \
    -i data/input/swim.json
```

## Input

This repository uses the same JSON format as [`jagua-rs`](https://github.com/JeroenGar/jagua-rs) to represent instances.

sparrow 0.3.0 uses jagua-rs 1.0.0 from crates.io. This release changes the input
format: every item requires `orientation.rotation`:
`{"mode":"discrete","angles":[0,180]}`, `{"mode":"stepped","step":90}`, or
`{"mode":"continuous"}`. Item IDs may be non-consecutive and are preserved in output.
Set `min_item_separation` on the input instance; omission means zero. The CLI and
solver configuration no longer override it. Legacy orientation fields and item
holes are rejected. See the [1.0.0 release notes](https://github.com/JeroenGar/jagua-rs/blob/v1.0.0/docs/releases/1.0.0.md).
These are also available in Oscar Oliveira's [OR-Datasets repository](https://github.com/Oscar-Oliveira/OR-Datasets/tree/master/Cutting-and-Packing/2D-Irregular).

See [`jagua-rs` README](https://github.com/JeroenGar/jagua-rs?tab=readme-ov-file#input) for details on the input format.

## Output

Solutions are exported as SVG files in the `output` folder. 
The final SVG solution is saved as `output/final_{name}.svg`.

The SVG files serve both as a visual and exact representation of the solution.
All original shapes and their exact transformations applied to them are defined within the SVG:
```html
    ...
    <g id="items">
        <defs>
            <g id="item_0">...</g>
        </defs>
        <use transform="translate(1289.9116 1828.7717), rotate(-90)" xlink:href="#item_0">...</use>
    </g>
    ...
```
The [SVG spec](https://stackoverflow.com/questions/18582935/the-applying-order-of-svg-transforms) defines that the transformations are applied from right to left.
So here the item is always first rotated and then translated.

By default, a range of intermediate (and infeasible) solutions will be exported in `output/sols_{name}`.
To disable this and export only a single final solution, compile with the `only_final_svg` feature:
```bash
cargo run --release --features=only_final_svg -- \
    -i data/input/swim.json
```
The final solution is saved both in SVG and JSON format in `output/final_{name}.svg` and `output/final_{name}.json`, respectively.

## Targeting maximum performance

This crate is highly optimized and is floating-point heavy.
To enable the maximum performance, make sure `target-cpu=native` compiler flag is set, 
switch to the nightly toolchain (required for [SIMD](https://doc.rust-lang.org/std/simd/index.html) support) 
and enable the `simd` feature:

```bash
  export RUSTFLAGS='-C target-cpu=native'
  export RUSTUP_TOOLCHAIN=nightly
  cargo run --release --features=simd,only_final_svg -- \
      -i data/input/swim.json
```

## Testing
A suite of `debug_assert!()` checks are included throughout the codebase to verify the correctness of the heuristic.
These assertions are omitted in release builds to maximize performance, but are active in test builds.
Some basic integration tests are included that run the heuristic on a few example instances while all assertions are active:
```bash
  cargo test
```

Alternatively you can enable all `debug_assert!()` checks in release builds by running the tests with the `debug-release` profile:
```bash
cargo run --profile debug-release -- \
    -i data/input/swim.json
```

## Experiments
All solutions from the comparative experiments in the paper can be found at
[data/experiments](data/experiments).
The accompanying [README](data/experiments/README.md) details how to perform an exact reproduction of any benchmark run.

## Related Projects

- [`sparrow/studio`](https://github.com/JeroenGar/sparrow-studio): an interactive browser interface for `sparrow`
- [`spyrrow`](https://github.com/PaulDL-RS/spyrrow): a Python wrapper of `sparrow`
- [`sparrow-3d`](https://github.com/JonasTollenaere/sparrow-3d): a 3D adaptation of `sparrow`

## Development

This repo is meant to remain a faithful representation of the algorithm described in the paper.
However, I am open to pull requests containing bug fixes and speed/performance improvements as long as they do not alter the algorithm too significantly.

Feel free to fork the repository if you want to experiment with different heuristics or want to expand the functionality.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgements

<p>
<img src="https://upload.wikimedia.org/wikipedia/commons/4/49/KU_Leuven_logo.svg" height="30px" alt="KU Leuven logo">
&nbsp;
<img src="https://upload.wikimedia.org/wikipedia/commons/9/97/Fonds_Wetenschappelijk_Onderzoek_logo_2024.svg" height="30px" alt="FWO logo">
</p>


This project began development at the CODeS research group of [NUMA - KU Leuven](https://numa.cs.kuleuven.be/) and was funded by [Research Foundation - Flanders (FWO)](https://www.fwo.be/en/) (grant number: 1S71222N) until October 2025.

Since then, I have continued the development and maintenance of the project independently.
If you find `sparrow` useful and would like to support its continued development and maintenance, you can do so through [GitHub Sponsors](https://github.com/sponsors/JeroenGar) or [Buy Me a Coffee](https://buymeacoffee.com/jeroengards).
