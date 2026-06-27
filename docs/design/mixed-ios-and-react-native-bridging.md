# 混合 iOS + React Native 桥接 - 覆盖设计

**观众：** 克劳德特工（或人类）在 #165 着陆后继续这项工作
纯 Objective-C 支持。
**任务：**制作 rustcodegraph 的 `trace` / `callers` / `callees` / `impact` /
流上下文调用跨**跨语言运行时进行端到端连接
调度边界**如今悄然打破了流程：**Swift ↔ Objective-C**
在混合 iOS 代码库中，以及 React Native / Expo 中的 **JavaScript ↔ native**
应用程序。

> 本文档是**计划**，而不是实施。没有代码登陆此
> 分支——只有设计、验证语料库和成功栏。
> 编码从每个阶段的后续分支开始。

这项工作是
[动态调度覆盖手册](./dynamic-dispatch-coverage-playbook.md) §6
矩阵：“Swift × Objective-C 桥接”行和一个新的“React Native 桥接”
排。两者都是 **resolver** 模式（双方都存在名为 refs 的
桥接规则是确定性的）——而不是合成器模式。参见第 3a 节
参考 Django ORM 解析器的 playbook。

---

## 1. 为什么这很重要（今天的差距）

#165 之后，rustcodegraph 索引了 Swift、Objective-C 和 JavaScript/TypeScript
每个都正确地**隔离**。但价值在于跨语言流动——
iOS 应用程序和 React Native 应用程序所在的位置：

- **混合 iOS 应用程序：** `MyViewController.swift` 调用 `imageDownloader.download(url:completion:)`，
也就是 `ImageDownloader.m` 中的 `-[ImageDownloader downloadURL:completion:]`。
今天：`trace("MyViewController.viewDidLoad", "downloadURL:completion:")`
调用不返回路径。 Swift 调用点解析为 `call_expression`，其
选择器无处可去； ObjC方法作为一个没有传入的节点存在
边缘。代理读取这两个文件以重建网桥。
- **React Native 应用程序：** `useEffect(() => NativeModules.Geolocation.getCurrentPosition(cb))`
在 `App.js` 达到 `RCT_EXPORT_METHOD(getCurrentPosition:(RCTResponseSenderBlock)cb)`
在`RNCGeolocation.m`。今天：JS 调用点没有传出优势
ObjC 实现； ObjC 处理程序没有来自 JS 的传入边缘。
`impact(getCurrentPosition)`（ObjC 端）显示没有 JS 调用者。
- **世博模块：** `await ExpoCamera.takePictureAsync(options)` (JS) 到达
`ExpoCamera.swift` 中的 `AsyncFunction("takePictureAsync") { ... }`（世博会
模块 API）。同样的休息。

在每种情况下，代理人或名称匹配者都存在**双方的名称
can correlate — Swift 的自动桥接 ObjC 选择器，`RCT_EXPORT_METHOD`
文字第一个参数，Expo `Function("name")` 文字。修复方法是
**解析器**知道每个通道的桥接规则并发出
`references` 与 `provenance:'heuristic'` 和 `metadata.synthesizedBy:'<channel>'` 边缘。

剧本的承重警告在这里比平常更适用：

> **部分覆盖比没有覆盖更糟糕。** 弥合一个边界，但不弥合整个边界
> 接下来显示一个跳跃，然后代理进行钻探+读取以完成。始终关闭
> 端到端的流程并重新测量——切勿发送半桥流程。

对于混合 iOS，这意味着**两个方向**（Swift→ObjC 和 ObjC→Swift）并且
**所有桥接类型**（方法、属性、初始化/初始化器、协议）
测量前必须关闭。对于 React Native，JS→native AND
native→JS (`RCTEventEmitter`, `sendEvent`) 必须都关闭，并且**都关闭
传统桥和 TurboModules**，或混合它们的应用程序将半桥。

---

## 2. 建模的桥接机制

在剧本的词汇表中，每一行都是一个单独的**调度通道** -
每个都有自己的解析器（如果不存在静态引用则为合成器），它自己的
验证，在 §6 矩阵中拥有自己的行。

| # | 方向 | 渠道 | 映射规则 | 它住在哪里 | 困难 |
|---|---|---|---|---|---|
| 1 | Swift→ObjC | 直接调用，通过`-Bridging-Header.h`导入ObjC类 | Swift 调用 `obj.x(y:z:)` ↔ ObjC 选择器 `-x:z:` （文字映射，参见 §3a） | `frameworks/swift-objc.ts` 中的旋转变压器 | 中等的 |
| 2 | ObjC → Swift | `@objc`曝光 | Swift `@objc func foo(bar:)` ↔ ObjC `-fooWithBar:` （自动名称）； `@objc(custom:)` 覆盖 | `frameworks/swift-objc.ts` 中的旋转变压器 | 中等的 |
| 3 | 斯威夫特 ↔ ObjC | 属性/getter/setter 桥接 | 斯威夫特 `var name: String` ↔ 对象 C `-name` / `-setName:` | `frameworks/swift-objc.ts` 中的旋转变压器 | 低的 |
| 4 | 斯威夫特 ↔ ObjC | 初始化器桥接 | 斯威夫特 `init(name:age:)` ↔ ObjC `-initWithName:age:` | `frameworks/swift-objc.ts` 中的旋转变压器 | 低的 |
| 5 | 斯威夫特 ↔ ObjC | 协议桥接 (`@objc protocol`) | 跨语言的一致性边缘 | `frameworks/swift-objc.ts` 中的旋转变压器 | 中等的 |
| 6 | JS → ObjC（RN 遗留桥） | `NativeModules.<Mod>.<fn>` ↔ `RCT_EXPORT_METHOD(<fn>:...)` 或 `RCT_REMAP_METHOD(<jsName>, <selector>:...)` | ObjC 端由 `RCT_EXPORT_MODULE()` 文字键入的名称匹配 | `frameworks/react-native.ts` 中的旋转变压器 | 中等的 |
| 7 | JS → Java/Kotlin（RN 遗留桥，Android） | `NativeModules.<Mod>.<fn>` ↔ `ReactContextBaseJavaModule` 子类上的 `@ReactMethod` 带注释的方法，其中 `getName()` 返回 `<Mod>` | 解析器 — 形状与 #6 相同，JVM 端 | 中等的 |
| 8 | JS ↔ 原生（RN TurboModules / Codegen） | `TurboModuleRegistry.get('Mod')` ↔ 生成的规范接口（`NativeMod` TS 类型） ↔ 与规范匹配的 ObjC++/Kotlin impl | 将规范文件读取为基本事实的解析器 | 难的 |
| 9 | 原生 → JS（事件） | ObjC `[self sendEventWithName:@"x" body:b]`（扩展 `RCTEventEmitter`）↔ JS `new NativeEventEmitter(NativeModules.Mod).addListener('x', cb)` | EventEmitter 风格的合成器（与语言 EventEmitter 的现有 `callback-synthesizer.ts` 相匹配） | 中等的 |
| 10 | JS → 原生（Expo 模块） | JS `ExpoX.fn(args)` ↔ Swift `Function("fn") { ... }` 或 `AsyncFunction("fn") { ... }` 位于具有 `Name("ExpoX")` 的 `Module` 子类中 | `frameworks/expo-modules.ts` 中的旋转变压器 | 中等的 |
| 11 | JS → 原生（Fabric 视图组件） | JS `<MyView prop={v}/>` ↔ ObjC/Swift `RCT_EXPORT_VIEW_PROPERTY(prop, ...)` 或 Codegen 查看规范 | 解析器 + JSX hop（与现有的 JSX 合成器组合） | 硬（推迟） |

**难度**列驱动分阶段——参见§6。

### 2a.为什么这些是解析器，而不是合成器

在每一行中，**桥接规则都是由名称确定的**：
- Swift 的 `@objc` 曝光的是有记录的自动映射； `@objc(custom:)`
是显式覆盖；两者都是静态可提取的。
- `RCT_EXPORT_METHOD` 采用文字选择器； `RCT_EXPORT_MODULE()` 采取
可选的文字模块名称（默认值：类名称减去 `RCT` 前缀）；
`NativeModules.Mod.fn` 是对已知全局的文字属性访问。
- 展览模块 `Function("name") { ... }` 和 `Module { Name("ExpoX"); ... }`
是 `Module` 定义内的文字字符串。
- TurboModules 规范接口是文字 `Native<Name>` 导出
`TurboModuleRegistry.get<...>('<Name>')`。

所以工作是：**提取桥接端名称→使解析器匹配
他们**。与 `djangoResolver` 形状相同，将 `_iterable_class` 解析为
`ModelIterable` — 不需要全图相关传递。

一个例外是 **#9 本机→JS 事件**，其中注册站点
看起来非常像现有的语言内 EventEmitter 模式
回调合成器已经处理。扩展该合成器
跨语言渠道是天作之合。

---

## 3. 具体桥接规则（参考表）

### 3a. Swift → ObjC 选择器映射（自动）

Swift 使用标准规则从 Swift 方法派生 ObjC 选择器：

| 迅速声明 | 对象选择器 |
|---|---|
| `func greet()` | `greet` |
| `func say(_ msg: String)` | `say:` |
| `func set(name: String)` | `setWithName:` |
| `func setName(_ name: String)` | `setName:` |
| `func move(to point: CGPoint)` | `moveTo:` |
| `func move(from a: CGPoint, to b: CGPoint)` | `moveFrom:to:` |
| `init(name: String)` | `initWithName:` |
| `init(name: String, age: Int)` | `initWithName:age:` |
| `var name: String`（吸气剂） | `name` |
| `var name: String`（二传手） | `setName:` |
| `@objc(customSel:) func f(...)` | `customSel:`（显式覆盖） |

完整的规则集位于
[Apple — 将 Swift 导入 Objective-C](https://developer.apple.com/documentation/swift/importing-swift-into-objective-c)
— 特别是“方法名称翻译”和“初始化程序名称翻译”
部分。解析器在提取处以**一个方向实现此映射
time** （Swift 声明生成桥接 ObjC 名称，附加为
Swift 方法节点上的别名），因此 ObjC 端的名称解析找到
通过正常名称匹配的 Swift 方法。

### 3b. React Native 遗留桥 — 名称解析

```objc
// Native side (ObjC)
@implementation RCTGeolocation
RCT_EXPORT_MODULE();                                    // module name: "Geolocation" (RCT prefix stripped)
RCT_EXPORT_METHOD(getCurrentPosition:(RCTResponseSenderBlock)cb) { ... }
@end
```
```js
// JS side
import { NativeModules } from 'react-native';
NativeModules.Geolocation.getCurrentPosition(cb);       // resolves to the ObjC method above
```

规则：
1. 在本机端，为每个类提取一个合成 `module` 节点，其中包含
`RCT_EXPORT_MODULE()`。名称 = 显式字符串参数（如果存在），否则
去掉 `RCT` 前缀的类名。
2. 每个 `RCT_EXPORT_METHOD(<sel>)` 和 `RCT_REMAP_METHOD(<jsName>, <sel>)`
成为附加到该模块节点的方法节点，具有 JS 可见
名称（`<sel>` 的第一个关键字为 `RCT_EXPORT_METHOD`，或 `<jsName>` 为
`RCT_REMAP_METHOD`）。
3. 在 JS 端，解析器匹配文字属性链
`NativeModules.<Mod>.<fn>` 与 `(module, jsName)` 对
本机方面。
4. 旋转变压器发出 `references`（`provenance:'heuristic'`、`synthesizedBy:'rn-bridge'`）
从 JS 调用点到本机方法。

### 3c. React Native TurboModule — 名称解析

```ts
// Spec (TS) — codegen ground truth
export interface Spec extends TurboModule {
  getCurrentPosition(cb: (loc: Location) => void): void;
}
export default TurboModuleRegistry.getEnforcing<Spec>('Geolocation');
```
```objc
// ObjC++ impl
@implementation RCTGeolocation
- (void)getCurrentPosition:(RCTResponseSenderBlock)cb { ... }
@end
```
```js
import Geolocation from './NativeGeolocation';
Geolocation.getCurrentPosition(cb);  // resolves to the ObjC method via the spec
```

规则：
1. 规范文件是事实来源：解析 `TurboModuleRegistry.get*<Spec>('<Name>')`
找到模块名称，然后读取`Spec`接口方法。
2. 将每个规范方法与本机 impl 的同名方法相匹配（通过选择器
第一个关键字，在通过名称约定或阅读识别的类中
任何 `JSI_EXPORT_MODULE` 宏（如果存在）。
3. 规范文件的 JS 导入通过规范获得名称解析。
4. 发出与 #3b 相同的 `references` 边缘，其中 `synthesizedBy:'rn-turbomodule'`。

### 3d.世博模块 — 名称解析

```swift
// Native (Swift, expo-modules-core API)
public class ExpoCameraModule: Module {
  public func definition() -> ModuleDefinition {
    Name("ExpoCamera")
    AsyncFunction("takePictureAsync") { (options: CameraOptions) in /* ... */ }
    View(ExpoCameraView.self) {
      Prop("type") { (view: ExpoCameraView, type: String) in /* ... */ }
    }
  }
}
```
```js
import { requireNativeModule } from 'expo-modules-core';
const ExpoCamera = requireNativeModule('ExpoCamera');
await ExpoCamera.takePictureAsync({ quality: 1 });
```

规则：
1. 在本机端：扩展 `Module` 的类，其 `definition()` （或
`init { /* DSL */ }` 对于较新的 API）包含 `Name("X")` 调用定义
该模块。每个 `Function("y")` / `AsyncFunction("y")` 文字定义一个
方法。尾随闭包是实现主体——extract as a
名为 `y` 的方法节点，附加到模块 `X`。
2. 在JS端：`requireNativeModule('X')`产生一个绑定；解决
属性访问它的命名方法。
3. 视图模块的 `Prop("name")` 的行为类似于 RN 的 `RCT_EXPORT_VIEW_PROPERTY` —
与视图组件边界的其余部分推迟。

---

## 4. 需要存在哪些边

对于每个通道，闭合流量为：

- **JS 调用点 → 桥接方法节点**（`references`、启发式、`synthesizedBy:'<channel>'`）
- **桥接方法节点→本机实现方法**（已提取；适用于#6/#7
桥接方法是本机实现；对于#10，封闭体是
暗示）
- **Native-impl-method → 它自己的被调用者**（已用语言提取）

特别是对于 Swift↔ObjC，最干净的模型是 **alias-name
声明节点**：扩展 Swift 方法提取来计算 ObjC
自动桥接名称并将其存储为解析器的备用名称
认为。 Swift 和 ObjC 方法节点之间不需要新的边
— 正常的名称解析就足够了，因为双方都同意桥接
提取后的选择器。

MCP 读取工具已内联表面启发式边缘
（参见 #312/#403 中的 `metadata.synthesizedBy` 管道）；这些新的边缘
沿着这条路行驶，无需额外的管道。

---

## 5. 验证语料库（小/中/大栏）

遵循 CLAUDE.md 的验证方法 — **≥3 个流程提示
小型/中型/大型存储库，具有确定性探针+代理 A/B，
≥2 次运行/臂**。以下选择是要承诺的候选人
实施部门；实施 PR 确认之后的选择
验证每个存储库仍然可以干净地构建索引。

### 5a.混合 iOS (Swift+ObjC) — 选择 3

| 等级 | 回购协议 | 为什么 | 规范流 |
|---|---|---|---|
| **小的** | [图表](https://github.com/danielgindi/Charts)（约 150 个文件 Swift+ObjC） | 带有 ObjC 兼容层的 Swift-first 库；知名 | “在 `ChartView` 上设置 `data` 如何到达渲染器？” |
| **小（替代）** | [洛蒂奥斯](https://github.com/airbnb/lottie-ios)（约 300 个文件，混合在一起；当前可能是纯 Swift — 验证） | 动画引擎，知名组合 | “`AnimationView.play()` 如何到达图层合成器？” |
| **中等的** | [境界-可可](https://github.com/realm/realm-swift)（约 500 个文件） | Heavy Swift-on-top-of-ObjC：Swift API 包装了 ObjC 核心，而 ObjC 核心又包装了 C++ Realm Core | “`Realm.write { realm.add(obj) }`如何到达ObjC持久层？” |
| **大的** | [维基百科-iOS](https://github.com/wikimedia/wikipedia-ios)（约 2500 个 Swift+ObjC 文件） | 真正的应用程序，深度混合，积极开发 | “点击搜索结果如何到达文章获取网络调用？” |
| **大（替代）** | [WordPress-iOS](https://github.com/wordpress-mobile/WordPress-iOS) | 较重的 ObjC 遗留 + Swift 添加 | “新帖子草稿保存如何达到核心数据持久性？” |

每个仓库的酒吧：
1. 纯语言探测仍然通过（Swift-in-Swift 跟踪；ObjC-in-ObjC 跟踪）——与 #165 的纯 ObjC 基线相比没有回归。
2. **跨语言探测通过：** 上面的规范流程以 `trace` 端到端跟踪，在语言边界处没有中断。
3. **代理 A/B（使用 rustcodegraph 与不使用 rustcodegraph，≥2 次运行/臂）：** 在探索调用预算内读取 = 0；比不使用 rustcodegraph 更快；纯 Swift 或纯 ObjC 控制存储库（例如纹理）上没有回归。
4. **没有节点数爆炸** 与桥接前基线（之前/之后的 `select count(*) from nodes`）相比。

### 5b. React Native — 选择 3

| 等级 | 回购协议 | 为什么 | 规范流 |
|---|---|---|---|
| **小的** | [反应本机 svg](https://github.com/software-mansion/react-native-svg)（~100 个文件 JS+ObjC+Java） | 小型、范围广泛的本机模块集 | “设置`<Path d=.../>`如何到达iOS Core Graphics调用？” |
| **中等的** | [反应本机屏幕](https://github.com/software-mansion/react-native-screens)（~300 个文件，JS+原生） | 真正的导航原语，包括传统的网桥和 Fabric | “导航到新屏幕如何到达 UINavigationController？” |
| **中（替代）** | [反应本机 Firebase](https://github.com/invertase/react-native-firebase)（跨包约 1000 个文件） | 许多本机模块，两个平台 - 强调模块发现 | “`firestore().collection('x').get()` 如何到达 iOS Firebase SDK 调用？” |
| **大的** | [facebook/react-native](https://github.com/facebook/react-native) RNTester 子集（约 3000 个文件） | 框架本身+示例应用程序；规范桥接用法 | “按下 RNTester 的 GeolocationExample 中的按钮如何到达 iOS 核心位置调用？” |

每个仓库的酒吧：
1. Pure-JS 探针不变（`useState` → 重新渲染流程仍然解析 - 现有的反应合成器没有回归）。
2. **JS → ObjC 桥接探测通过**，每个存储库上有 ≥1 个已知的 RCT_EXPORT_METHOD。
3. **JS → TurboModule 探针在使用 TurboModules 的存储库上传递**（react-native main 两者都有；各选一个）。
4. **本机 → JS 事件探测传递**对于 ≥1 个发射器（NativeEventEmitter 模式）。
5. **代理 A/B** 如上所述。关键：*过桥*的问题（例如“按下按钮 X 如何到达网络调用”）必须在使用 rustcodegraph 运行 ≥1 次时将 Read 降至 0。
6. **在纯 JS 控制存储库上没有回归**（现有的 React-realworld / excalidraw 测量不变）。

### 5c. Expo — 选择 2（范围更小，API 面更窄）

| 等级 | 回购协议 | 为什么 |
|---|---|---|
| **小/中** | [博览会](https://github.com/expo/expo) — 一个 SDK 模块，如 `expo-camera` 或 `expo-location` | 最干净的 Expo Modules API 示例；居住 |
| **大的** | 完整的 `expo/expo` monorepo（所有 SDK 模块 + JS API） | 跨多个包的压力测试模块名称解析 |

规范流程：“`await Camera.takePictureAsync()` (JS) 如何到达
本机相机 API 调用（Swift `AVCaptureSession` 或 Kotlin
`CameraDevice`）？”

---

## 6. 分阶段——先做什么

根据剧本的难度梯度和半桥规则，顺序
由在**最小的存储库首先**上端到端关闭流程的内容来修复。

### 第 1 阶段 — Swift ↔ ObjC 桥接（上面第 1-5 行）
最小范围，确定性名称映射，不涉及 JS。验证在
继续之前的图表/领域/维基百科语料库。 **不要继续进入第 2 阶段
直到第 1 阶段通过所有三个存储库的 §5a 条。**

### 第 2 阶段 — React Native 遗留桥（第 6-7 行，ObjC + Java/Kotlin）
iOS 和 Android 双方必须在同一个 PR 中关闭——半桥接 PR
平台显示另一个平台上的半覆盖跳跃，并且代理读取。
在 §5b 语料库上进行验证。

### 第 3 阶段 — 本机 → JS 事件（第 9 行）
使用跨语言通道扩展现有的回调合成器。
在相同的 §5b 语料库上进行验证（大多数 RN 库至少使用一个事件发射器）。

### 第 4 阶段 — 世博模块（第 10 行）
分层于第一阶段的快速提取。较小的语料库 (§5c)。

### 第 5 阶段 — RN TurboModules/Codegen（第 8 行）
需要将规范文件作为跨语言的基本事实进行读取。验证于
§5b 语料库的 TurboModule 用户（react-native main，0.73 后的库）。

### 第 6 阶段 - Fabric 视图组件（第 11 行）
Deferred — 与现有的 JSX 合成器和视图端组合
涡轮增压模块。当 ≥1 个 §5b 语料库存储库有桥时的地址
否则关闭，但结构流仍然中断。

---

## 7. 反目标（我们不会尝试做的事情）

- **Android Kotlin/Java 提取质量** — 超出范围。我们用什么
Kotlin/Java 提取器已经生成。如果他们错过了 `@ReactMethod`
注释的字面名称我们可以添加一个微小的提取器细化，但是我们
不要重新设计 JVM 提取。
- **动态/计算桥键** — `NativeModules[someVar]`，
`requireNativeModule(name)` 其中`name`是参数等。我们只
解析文字键访问（匹配
[代理评估 Lua 前沿](./dynamic-dispatch-coverage-playbook.md) — 仅匿名模式被推迟）。
- **桥接头文件内容解析** — 我们*做*索引 `.h` 文件
（已经通过#165的内容嗅探做到了）但我们**不**解析
桥接标头的 `#import` 列表作为特殊的“Swift 可见内容”
显现。将其视为普通的 ObjC 标头。
- **`performSelector:` 上的运行时调度** — 超出范围；匹配
同样的“仅限命名”的反目标。
- **JSI（原始、非 TurboModule）** — 超出范围。使用裸 JSI 的应用程序
通过没有记录的自定义 `Host*` 接口调用本机
声明性规范等待这些应用程序迁移到 TurboModules。
- **ObjC 协议上的仅 Swift 泛型/ObjC 上的 Swift 扩展
类** — 如果 `@objc`，扩展方法仍然可以在 ObjC 中调用，所以
他们经历相同的第一阶段路径。泛型不是——我们默默地
想念他们。可以接受；匹配 Java/Kotlin 泛型前沿。

---

## 8. 覆盖矩阵条目——测量

| 语言 | 框架 | 规范流 | 机制 | 地位 |
|---|---|---|---|---|
| Swift × Objective-C | 桥接 | Swift 调用 → ObjC 选择器； ObjC 调用 → @objc Swift 方法 | 右 | ✅ 第 1 阶段 (§8a) |
| JavaScript × Objective-C/Java/Kotlin | React Native 遗留桥 | `NativeModules.<M>.<f>` → `RCT_EXPORT_METHOD` / `@ReactMethod` | 右 | ✅ 第 2 阶段 (§8b) |
| JavaScript × 原生 | React Native TurboModules | 规范接口 ↔ impl | R（规范作为基本事实） | ✅ 部分 — 名称匹配路径着陆 (§8b) |
| Objective-C/Java/Kotlin → JavaScript | React Native 事件发射器 | `[self sendEventWithName:]` → `addListener` | S（跨语言通道） | ✅ 第 3 阶段 (§8e) |
| JavaScript × Swift/Kotlin | 世博模块 | `requireNativeModule('X').fn(...)` → `Function("fn") { }` | R（提取合成方法节点） | ✅ 第 4 阶段 (§8f) |
| JavaScript × 原生 | React Native Fabric 视图 | `<MyView p=v/>` → Codegen 规范组件 + NativeProps | R（提取）+ S（原生实现）+ JSX | ✅ 第 6 阶段（§8g） |

### 8a.第一阶段测量 — Swift ↔ ObjC

| 回购协议 | 源文件 | 桥接边缘（框架解析） | 样品边缘 |
|---|---|---|---|
| **图表**（小） | 269（205 斯威夫特 + 59 ObjC/.h） | 28 objc→swift, 1 swift→objc | `handleOption:forChartView:` → `animate` · `setupPieChartView:` → `setExtraOffsets` · `setDataCount:range:` → `setColor` |
| **领域-快速**（中） | 369（151 Swift + 218 ObjC 系列） | 36 objc→斯威夫特, 1185 斯威夫特→objc | `valueForUndefinedKey:` → `get` · `setValue:forUndefinedKey:` → `set` · `promote:on:` → `initialize` |
| **维基百科-ios**（大） | 1734（1234 斯威夫特 + 500 ObjC/.h） | 52 objc→斯威夫特, 983 斯威夫特→objc | 真实 iOS 应用程序跨多个功能模块的桥接 |

所有这三个：语言基线不变，没有节点数爆炸，
`trace` 连接跨边界的规范流（已在
图表：`trace(handleOption:forChartView:, animate)` 表面
直接桥接边缘）。

### 8b.第 2 + 5 阶段（部分）测量 — React Native 桥

| 回购协议 | 源文件 | 桥接边缘（框架解析） | 笔记 |
|---|---|---|---|
| **react-native-svg**（小/中） | 〜700（93 .mm + 115 .java + 6 .kt + 49 js + 92 ts + 154 tsx） | 9 tsx→java 通过 TurboModule 规范 | RNSvg 的 iOS 使用 TurboModule auto-gen （无 `RCT_EXPORT_METHOD`）；决议涉及爪哇岛。所有 9 个精确：`isPointInStroke`、`isPointInFill`、`getTotalLength`、`getPointAtLength`、`getCTM`、`getScreenCTM`、`getBBox`、`toDataURL`。 |
| **AsyncStorage**（小型、纯遗留桥） | ~60（28 kt + 2 mm + 16 ts + 14 tsx + …） | **8/8 精确** | 规范的遗留桥接测试 — Kotlin `@ReactMethod` + ObjC `RCT_EXPORT_METHOD`。 JS `setItem` → Kotlin `legacy_multiSet`； `getItem` → `legacy_multiGet`； `clear` → `legacy_clear`； ETC。 |
| **react-native-firebase**（大） | 〜1100（111 .java + 63 .m + 13 .mm + 239 js + 427 ts + 9 tsx） | RCTEventEmitter 黑名单后为 18（之前为 78） | 最初的 78 个包括 60 个针对 `addListener:` / `remove:` 的误报（每个 RCTEventEmitter 都声明它们；每个对 `.addListener(...)` 的 JS 调用都解析为噪声）。黑名单削减至 18 个，全部精确：`httpsCallable:region:emulatorHost:...`、`signInWithProvider`、`configureProvider`、`removeFunctionsStreaming:`。 |
| **反应本机屏幕**（中） | 1211 | 0 — 空 TurboModule 规范，无 `RCT_EXPORT_METHOD`，所有 Fabric/Codegen 视图端 | RNScreens 完全处于第 6 阶段（Fabric，延迟）。桥牌在这里拒绝过度匹配是正确的行为。 |

### 8c.验证期间发现的架构修复

解析器的 `initialize()` 在 RustCodeGraph 构造中运行 - 在任何之前
文件被索引 - 因此 `detect()` 咨询的框架解析器
索引文件列表（UIKit / SwiftUI 扫描导入，
`swift-objc-bridge` 寻找 Swift 和 ObjC 文件，
`react-native-bridge` 寻找 RN 标记）全部返回 false
最初的通过并默默地放弃了自己。这影响了每一个
代码库中的框架解析器读取 `context.getAllFiles()` /
`context.readFile()` 而不是直接扫描文件系统 - a
预先存在的潜在错误，不是特定于桥的。已修复：现在 `indexAll()`
提取完成后调用 `resolver.initialize()`，因此 detector()
针对填充的索引运行。

### 8d.桥接精度阻止列表（经验教训）

| 桥 | 被屏蔽的名字 | 原因 |
|---|---|---|
| swift-objc | `init`、`description`、`hash`、`isEqual`、`copy`、`count`、`value`、`data`、`string`、`object`、`add`、`remove`、`update`、`load`、 `save`、`reload`、`cancel`、`start`、`stop`、`pause`、`resume`、`close`、`open`、`show`、`hide`、`dealloc`、`release`、 `retain`、`autorelease`、…… | 每个 NSObject 子类都实现了这些；将它们桥接到任意项目本地 ObjC 方法会产生噪音。常规名称匹配器会自行处理它们。 |
| 反应本机 | `addListener`、`removeListeners`、`remove`、`invalidate`、`startObserving`、`stopObserving` | 每个 `RCTEventEmitter` 子类都通过 `RCT_EXPORT_METHOD` 声明它们。 `.addListener(...)` / `.remove(...)` 的 JS 调用者通过 `NativeEventEmitter` （JS 抽象），而不是直接通过本机桥。 |

### 8e.第 3 阶段测量 — RN 原生 → JS 事件通道

合成器模式；将 `src/resolution/callback-synthesizer.ts` 扩展为
由文字事件名称键入的跨语言事件通道。验证于
**RNFirebase**（大）：

| 合成事件通道 | 边缘 | 样本 |
|---|---|---|
| `messaging_message_received` | 2 | `application:didReceiveRemoteNotification:fetchCompletionHandler:` → TS `onMessage`（并且 `UNUserNotificationCenter` 将呈现变体 → 相同的 `onMessage`） |
| `messaging_notification_opened` | 1 | `userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:` → TS `onNotificationOpenedApp` |

每条边都是`provenance:'heuristic'`，
`metadata.synthesizedBy:'rn-event-channel'`。相同`EVENT_FANOUT_CAP = 6`
作为语言通道 - 具有太多处理程序的通用事件名称
或者调度员跳过而不是过度链接。

合成器还处理常见的 **订阅包装模式**
RN 库（`messaging().onMessage(listener)`，其中 `listener` 是
流向用户代码的参数）：当 JS 处理程序 arg 不是
命名符号，它将侦听器归因于 ENCLOSING JS 函数
（可达性正确，抽象层的属性）。

### 8f.第四阶段测量——Expo 模块

框架 `extract()` 解析 Swift / Kotlin 源代码以获取文字
`Function("X") { … }` / `AsyncFunction("X") { … }` / `Property("X") { … }`
/ `Constants` 里面的声明 `class X: Module` （或者 `: Module()` 里面
Kotlin) 并为每个文字发出一个名为 `X` 的 `method` 节点。标准
name-matcher 将 `Foo.takePictureAsync(...)` 之类的 JS 调用解析为
这些合成节点通过现有的 `obj.method` → 方法名称路径。

在真实的 Expo SDK 包上进行验证：

| 包裹 | 文件已编入索引 | 提取的 Expo 方法节点 | 跨语言边缘 |
|---|---|---|---|
| **世博会触觉** | 14 | 6（3 Swift + 3 Kotlin：`notificationAsync`、`impactAsync`、`selectionAsync` / `performHapticsAsync`） | 模块节点注册；消费者应用程序调用者通过名称匹配进行解析 |
| **世博相机** | 72 | 41（Swift + Kotlin；涵盖 `takePictureAsync`、`record`、`resumePreview`、`getAvailableLenses`、`scanFromURLAsync`、`requestCameraPermissionsAsync`、视图端 `width` / `height` 属性，...） | 9 swift→expo，7 kotlin→expo 内部边缘。包中的 JS 端调用点使用 TS 包装器隐藏本机名称（在 `CameraView.tsx` 上定义的 `pausePreview()`）； name-match 正确地更喜欢本地 TS 方法。 `Camera.takePictureAsync()` 的外部消费者应用程序直接解析为本机方法。 |

五项测试涵盖提取器 + 端到端固定装置：
文字 AsyncFunction("uniqueExpoHapticCall") 的 JS 调用点解析
到本机 impl 节点` — 确认无解析器的桥接路径
当名称没有阴影时有效。

### 8克。第 6 阶段测量——Fabric/Codegen 视图组件

两部分设计：

1. **框架提取器** (`src/resolution/frameworks/fabric.ts`) — 解析
`codegenNativeComponent<Props>('Name', ...)` 的 TS / TSX 规格文件
声明。发出：
   - 每个声明一个 `component` 节点（以 JS-visible 命名）
组件名称；匹配 JSX 合成器的 name+kind 过滤器）。
   - `NativeProps` 的每个声明字段有一个 `property` 节点
接口 — 呈现 JSX 可调用的 props，例如 `onTap`，
`nativeContainerBackgroundColor` 作为可发现的图节点。

2. **合成器**（`callback-synthesizer.ts` 中的 `fabricNativeImplEdges`）—
遍历每个 `fabric-component:*` 节点并寻找本地类
将其名称与 RN 的约定后缀之一匹配（空 / `View`
/ `ViewManager` / `ComponentView` / `Manager`）。发出 `calls` 边缘
组件中的 `metadata.synthesizedBy:'fabric-native-impl'`
每场比赛。约定足够精确，以至于没有名字
结构良好的 RN 库中的碰撞。

与现有的 `reactJsxChildEdges` JSX 合成器相结合，该
关闭完整的 JSX → 原生流程：消费者应用程序 JSX `<MyView prop=v/>`
→ Fabric `component` 节点 `MyView` → 原生类 `MyViewView`
（或 `MyViewManager` / `MyViewComponentView` / …）。

在**react-native-screens**（语料库存储库）上重新验证
完全 Fabric 并在第 2 阶段显示 0 个桥）：

| 公制 | 数数 |
|---|---|
| `codegenNativeComponent` 规格声明 | 54 |
| 提取的 Fabric 组件节点 | 27（每个非网络规范一个；`*.web.ts` 变体按规范有效性过滤掉） |
| 提取的 Fabric 支撑节点 | 272（所有组件的完整 NativeProps 界面） |
| `fabric-native-impl` 桥边 | 68 |

桥边缘示例：

| JS组件 | 母语班 | 后缀 |
|---|---|---|
| `RNSFullWindowOverlay` | `RNSFullWindowOverlay`（对象） | （精确的） |
| `RNSFullWindowOverlay` | `RNSFullWindowOverlayManager`（对象） | `Manager` |
| `RNSModalScreen` | `RNSModalScreenManager`（对象） | `Manager` |
| `RNSScreenContainer` | `RNSScreenContainerView`（对象） | `View` |

四项测试涵盖提取器 + 完整的端到端夹具
(`App (TSX) → MyView (fabric-component) → MyViewView (ObjC class)`)
断言 JSX→组件边缘 AND
组件→本机类边在索引后都存在。

---

## 9. 第一阶段需要解决的开放性问题

这些并不会阻碍第一阶段的开始——它们是首先要做的事情
*在*编写 Swift↔ObjC 解析器时决定：

1. **声明上的别名与新桥接边缘？** 存储自动桥接
ObjC 选择器作为 Swift 方法节点上的替代名称更便宜
并与名称解析的工作方式保持一致。另一种选择
（在匹配节点之间合成跨语言`references`边）
在 `trace` 输出中更明确，但为每个 `@objc` 符号添加 N 个边。
**默认：alias。** 验证 `callers`/`callees`/`trace` 中的别名表面
结果。
2. **`trace` 如何显示跨语言跳转？** MCP `trace` 工具
内联每个跃点的主体。 Swift → ObjC 跳跃应该使这一点显而易见
渲染的输出（“Swift `func foo(bar:)` →桥接到 ObjC 选择器
`-fooWithBar:` → ObjC `-[ImageDownloader fooWithBar:]`”）。可能会
需要在 `trace.ts` 中对渲染器进行小的调整来标记桥。
3. **解析器桥接规则位于哪里？** 建议
`src/resolution/frameworks/swift-objc.ts` 用于自动名称映射（
纯函数）由 Swift 提取器导入（以计算
提取时的别名）和测试。将映射保留在一处。
4. **`@objcMembers` 怎么样？** 类级别导出 — 适用于所有成员
除非`@nonobjc`。通过检查 Swift 中类的修饰符来处理
提取器并从中默认每个成员的 `@objc`-ness。

---

## 10. 完成吧（这样我们就知道何时停止）

第 1 阶段 (Swift↔ObjC) 在以下情况下完成：
- 所有三个 §5a 语料库均通过：纯语言探测不变；跨语言
规范流探针找到端到端的路径；代理 A/B 显示 Read = 0
使用 rustcodegraph 运行 ≥1 次，比不使用更快。
- 手册第 6 节中的覆盖矩阵行用数字填充。
- 存在 CHANGELOG `[Unreleased]` 条目，是在用户端写入的。

每个后续阶段都有相同的形状 - 它自己的 §5 语料库，它自己的
矩阵行，它自己的 CHANGELOG 条目 — 并且 ** 直到
前一项通过**。在这里，半桥是必须避免的；他们
积极地使 rustcodegraph 在这些代码库上比没有任何代码更糟糕
根本无法桥接。
