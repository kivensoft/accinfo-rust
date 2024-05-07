# accinfo
账户信息管理系统

---
#### 介绍
**keepass**是一款非常好用的账户信息管理工具，可是keepass是本地gui应用，安全性是有保障了，但无法通过远程或者手机进行访问，本工具正是为了解决该问题而产生的。

使用本工具，可对keepass导出的xml数据进行加密转换保存，对外提供http访问接口。aidb的数据做了加密保存，因此用户可以放心的将服务托管到云平台而不必担心数据泄漏，配合nginx做https反向代理，实现端到端的加密，可以安全、方便的为用户提供数据查询功能。

#### 项目地址
<https://github.com/kivensoft/accinfo_rust>

###### 技术框架

|名称|类型|选择理由|
|----|----|--------|
|rust|开发语言|安全、高性能、跨平台|
|tokio|异步io运行时|最流行、高性能|
|hyper|http协议|最流行、高性能|
|serde_json|json序列化|最流行、高性能|
|log|日志门面|最流行、官方推荐|
|chrono|日期时间处理|最流行|
|aes|数据库加密算法|高安全性|
|md5|口令加密算法|高安全性|
|asynclog|异步日志|简单、高性能|
|httpserver|http服务框架|简单、高性能|

###### 源代码下载
`git clone git@github.com:kivensoft/accinfo_rust.git`
###### 编译
`cargo build`
###### 运行
1. 导出keepass的数据库，导出类型为xml（假设导出文件名为simple.xml）
2. 转换xml为aidb并进行加密保存, 密码 12345678
   `accinfo -d simple.aidb -p 12345678 --encrypt simple.xml`
3. 启动应用
   `accinfo -L debug -d simple.aidb`
4. 打开浏览器，访问 `http://localhost:8080/`
