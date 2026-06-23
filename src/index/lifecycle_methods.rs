//! `CodeGraph` 生命周期方法：初始化、打开、关闭和反初始化。
//!
//! facade 会在入口处完成路径解析和 `.rustcodegraph` 目录校验，确保后续模块都接收绝对项目根目录。

use super::*;

impl CodeGraph {
    pub fn init(
        project_root: impl AsRef<Path>,
        options: InitOptions,
    ) -> Result<Self, CodeGraphError> {
        let resolved_root = resolve_root(project_root);
        if is_initialized(&resolved_root) {
            return Err(CodeGraphError::new(
                format!(
                    "RustCodeGraph already initialized in {}",
                    resolved_root.display()
                ),
                "ALREADY_INITIALIZED",
                None,
            ));
        }
        create_directory(&resolved_root)?;
        let db = DatabaseConnection::initialize(facade_database_path(&resolved_root))
            .map_err(|err| database_error("initialize", err))?;
        let mut instance = Self::new(resolved_root, db);
        if options.index {
            // 初始化时的索引失败不阻断实例创建，调用方仍可查看错误状态或稍后重跑 index。
            let _ = instance.index_all(IndexOptions::default());
        }
        Ok(instance)
    }

    pub fn init_sync(project_root: impl AsRef<Path>) -> Result<Self, CodeGraphError> {
        Self::init(project_root, InitOptions { index: false })
    }

    pub fn open(
        project_root: impl AsRef<Path>,
        options: OpenOptions,
    ) -> Result<Self, CodeGraphError> {
        let resolved_root = resolve_root(project_root);
        if !is_initialized(&resolved_root) {
            return Err(CodeGraphError::new(
                format!(
                    "RustCodeGraph not initialized in {}. Run init() first.",
                    resolved_root.display()
                ),
                "NOT_INITIALIZED",
                None,
            ));
        }
        let validation = validate_directory(&resolved_root);
        if !validation.valid {
            return Err(CodeGraphError::new(
                format!(
                    "Invalid RustCodeGraph directory: {}",
                    validation.errors.join(", ")
                ),
                "INVALID_DIRECTORY",
                None,
            ));
        }
        let db = DatabaseConnection::open(facade_database_path(&resolved_root))
            .map_err(|err| database_error("open", err))?;
        let mut instance = Self::new(resolved_root, db);
        if options.sync && !options.read_only {
            // open 的自动同步是便利路径；失败不应让只读查询场景无法打开已有索引。
            let _ = instance.sync(IndexOptions::default());
        }
        Ok(instance)
    }

    pub fn open_sync(project_root: impl AsRef<Path>) -> Result<Self, CodeGraphError> {
        Self::open(project_root, OpenOptions::default())
    }

    pub fn is_initialized(project_root: impl AsRef<Path>) -> bool {
        is_initialized(resolve_root(project_root))
    }

    fn new(project_root: PathBuf, db: DatabaseConnection) -> Self {
        Self {
            project_root,
            db,
            indexing: false,
            watching: false,
            watch_registered: false,
            watch_stop: None,
            watch_thread: None,
            watcher: None,
            watcher_degraded_reason: None,
            pending_files: Vec::new(),
            index_build_info: IndexBuildInfo::default(),
        }
    }

    pub fn close(&mut self) {
        // watcher 持有后台线程和底层文件监听句柄，关闭数据库前必须先停止。
        self.unwatch();
        let _ = self.db.close();
    }

    pub fn get_project_root(&self) -> String {
        self.project_root.to_string_lossy().into_owned()
    }
    pub fn destroy(&mut self) {
        self.close();
    }

    pub fn uninitialize(&mut self) -> Result<(), CodeGraphError> {
        self.close();
        remove_directory(&self.project_root)
    }
}
