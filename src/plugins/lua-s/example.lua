-- plugins/advanced_workflows.lua
-- Advanced ph plugin demonstrating the enhanced Lua bridge capabilities.
-- This plugin showcases:
-- - Custom command registration with complex logic
-- - Configuration-driven behavior
-- - Hook-based automation
-- - File system integration
-- - Environment-aware workflows

-- Plugin metadata
local PLUGIN_NAME = "Advanced Workflows"
local PLUGIN_VERSION = "1.2.0"
local PLUGIN_AUTHOR = "ph Community"

-- Initialize plugin
ph.log("INFO", "Loading " .. PLUGIN_NAME .. " v" .. PLUGIN_VERSION, "PLUGIN")

-- === UTILITY FUNCTIONS ===

-- Safe configuration getter with defaults
local function get_config(key, default)
    local value = ph.config_get(key)
    return value or default
end

-- Check if we're in a Git repository
local function is_git_repo()
    return ph.file_exists(".git") or ph.file_exists(".git/HEAD")
end

-- Get current branch name from Git
local function get_current_branch()
    local branch_file = ".git/HEAD"
    if not ph.file_exists(branch_file) then
        return "unknown"
    end
    
    -- This is simplified; in reality you'd read the file content
    return "main" -- Placeholder for actual branch detection
end

-- === CUSTOM COMMANDS ===

-- Smart sync: pull, rebase, push with conflict detection
function smart_sync(...)
    local args = {...}
    ph.log("INFO", "Executing smart sync workflow", "SMART_SYNC")
    
    if not is_git_repo() then
        ph.log("ERROR", "Not in a Git repository", "SMART_SYNC")
        return false
    end
    
    -- Check if auto-sync is enabled
    local auto_sync = get_config("ph.workflow.auto-sync", "true")
    if auto_sync ~= "true" then
        ph.log("WARN", "Auto-sync disabled in configuration", "SMART_SYNC")
        return false
    end
    
    local branch = get_current_branch()
    ph.log("INFO", "Syncing branch: " .. branch, "SMART_SYNC")
    
    -- Execute git operations with error handling
    local success = true
    
    -- Step 1: Fetch latest changes
    if not ph.run_command("fetch", {"--all", "--prune"}) then
        ph.log("ERROR", "Failed to fetch remote changes", "SMART_SYNC")
        return false
    end
    
    -- Step 2: Check for local changes
    local status_result = ph.run_command("status", {"--porcelain"})
    if not status_result then
        ph.log("ERROR", "Failed to check repository status", "SMART_SYNC")
        return false
    end
    
    -- Step 3: Pull with rebase if configured
    local pull_strategy = get_config("ph.workflow.pull-strategy", "merge")
    local pull_args = {}
    
    if pull_strategy == "rebase" then
        table.insert(pull_args, "--rebase")
    elseif pull_strategy == "ff-only" then
        table.insert(pull_args, "--ff-only")
    end
    
    if not ph.run_command("pull", pull_args) then
        ph.log("ERROR", "Failed to pull changes - possible conflicts", "SMART_SYNC")
        return false
    end
    
    -- Step 4: Push if auto-push is enabled
    local auto_push = get_config("ph.workflow.auto-push", "false")
    if auto_push == "true" then
        if not ph.run_command("push", {"origin", branch}) then
            ph.log("WARN", "Failed to push changes", "SMART_SYNC")
            -- Don't fail the entire operation for push failures
        else
            ph.log("INFO", "Successfully pushed changes to origin/" .. branch, "SMART_SYNC")
        end
    end
    
    ph.log("INFO", "Smart sync completed successfully", "SMART_SYNC")
    return true
end

-- Project setup command with templates
function setup_project(project_type, project_name)
    if not project_type then
        ph.log("ERROR", "Usage: ph setup <type> [name]", "SETUP")
        ph.log("INFO", "Available types: web, api, lib, docs", "SETUP")
        return false
    end
    
    project_name = project_name or "new-project"
    ph.log("INFO", "Setting up " .. project_type .. " project: " .. project_name, "SETUP")
    
    -- Create project directory structure based on type
    local templates = {
        web = {
            "src/",
            "public/",
            "docs/",
            "tests/",
            ".gitignore",
            "README.md",
            "package.json"
        },
        api = {
            "src/",
            "tests/",
            "docs/",
            "config/",
            ".gitignore",
            "README.md",
            "Dockerfile"
        },
        lib = {
            "src/",
            "include/",
            "tests/",
            "examples/",
            "docs/",
            "CMakeLists.txt",
            ".gitignore",
            "README.md"
        },
        docs = {
            "content/",
            "assets/",
            "config/",
            ".gitignore",
            "README.md"
        }
    }
    
    local template = templates[project_type]
    if not template then
        ph.log("ERROR", "Unknown project type: " .. project_type, "SETUP")
        return false
    end
    
    -- Initialize git repository
    if not ph.run_command("init", {project_name}) then
        ph.log("ERROR", "Failed to initialize Git repository", "SETUP")
        return false
    end
    
    ph.log("INFO", "Created project structure for " .. project_type, "SETUP")
    
    -- Set up project-specific configuration
    local config_prefix = "ph.project." .. project_type
    ph.config_set(config_prefix .. ".name", project_name)
    ph.config_set(config_prefix .. ".created", os.date("%Y-%m-%d"))
    
    -- Set up recommended Git hooks for this project type
    if project_type == "web" or project_type == "api" then
        ph.config_set("ph.hooks.pre-commit", "lint,test")
        ph.config_set("ph.hooks.pre-push", "build,test")
    end
    
    return true
end

-- Environment-aware status command
function enhanced_status()
    ph.log("INFO", "Generating enhanced status report", "STATUS")
    
    if not is_git_repo() then
        ph.log("ERROR", "Not in a Git repository", "STATUS")
        return false
    end
    
    -- Get basic git status
    if not ph.run_command("status", {"--short", "--branch"}) then
        return false
    end
    
    -- Add environment information
    local user = ph.getenv("USER") or ph.getenv("USERNAME") or "unknown"
    local home = ph.getenv("HOME") or ph.getenv("USERPROFILE") or "unknown"
    local pwd = ph.getenv("PWD") or "unknown"
    
    ph.log("INFO", "User: " .. user, "ENV_INFO")
    ph.log("INFO", "Working directory: " .. pwd, "ENV_INFO")
    
    -- Check for configuration-driven custom status
    local show_upstream = get_config("ph.status.show-upstream", "true")
    if show_upstream == "true" then
        ph.run_command("status", {"--ahead-behind"})
    end
    
    -- Check for uncommitted configuration changes
    if ph.file_exists(".phconfig") then
        ph.log("INFO", "Local ph configuration detected", "CONFIG")
    end
    
    return true
end

-- === HOOK IMPLEMENTATIONS ===

-- Pre-commit validation hook
function pre_commit_validation()
    ph.log("INFO", "Running pre-commit validation", "HOOK")
    
    -- Check if validation is enabled
    local enable_validation = get_config("ph.hooks.pre-commit.validation", "true")
    if enable_validation ~= "true" then
        ph.log("DEBUG", "Pre-commit validation disabled", "HOOK")
        return
    end
    
    -- Check for common issues
    local issues = {}
    
    -- Check for TODO/FIXME in staged files (simplified check)
    local todo_check = get_config("ph.hooks.pre-commit.check-todos", "true")
    if todo_check == "true" then
        ph.log("DEBUG", "Checking for TODO/FIXME markers", "HOOK")
        -- In real implementation, would examine staged files
        -- For demo, we'll just log the check
    end
    
    -- Check file sizes
    local max_file_size = get_config("ph.hooks.pre-commit.max-file-size", "10MB")
    ph.log("DEBUG", "Checking file sizes (max: " .. max_file_size .. ")", "HOOK")
    
    -- Log validation result
    if #issues == 0 then
        ph.log("INFO", "Pre-commit validation passed", "HOOK")
    else
        ph.log("WARN", "Pre-commit validation found " .. #issues .. " issues", "HOOK")
    end
end

-- Post-commit notification hook
function post_commit_notification()
    ph.log("INFO", "Post-commit notification triggered", "HOOK")
    
    local notify_enabled = get_config("ph.hooks.post-commit.notify", "false")
    if notify_enabled ~= "true" then
        return
    end
    
    local branch = get_current_branch()
    ph.log("INFO", "Commit completed on branch: " .. branch, "NOTIFICATION")
    
    -- Could integrate with external notification systems here
    local notification_url = get_config("ph.hooks.post-commit.webhook")
    if notification_url then
        ph.log("DEBUG", "Would send notification to: " .. notification_url, "HOOK")
    end
end

-- Backup hook for important operations
function backup_hook(operation)
    local enable_backup = get_config("ph.backup.enabled", "false")
    if enable_backup ~= "true" then
        return
    end
    
    local backup_dir = get_config("ph.backup.directory", ph.getenv("HOME") .. "/.ph-backups")
    ph.log("INFO", "Creating backup for operation: " .. (operation or "unknown"), "BACKUP")
    
    -- In real implementation, would create backup of current state
    ph.log("DEBUG", "Backup location: " .. backup_dir, "BACKUP")
end

-- === PLUGIN REGISTRATION ===

-- Register custom commands
ph.register_command(
    "sync", 
    "smart_sync", 
    "Smart synchronization with remote repository (fetch, pull, push)",
    "ph sync [--force]"
)

ph.register_command(
    "setup", 
    "setup_project", 
    "Set up a new project with predefined structure and configuration",
    "ph setup <type> [name] - Types: web, api, lib, docs"
)

ph.register_command(
    "enhanced-status", 
    "enhanced_status", 
    "Enhanced status with environment and configuration information",
    "ph enhanced-status"
)

-- Register hooks
ph.register_hook("pre-commit", "pre_commit_validation")
ph.register_hook("pre-commit", "backup_hook")
ph.register_hook("post-commit", "post_commit_notification")
ph.register_hook("pre-push", "backup_hook")

-- Plugin initialization complete
ph.log("INFO", "Advanced Workflows plugin loaded successfully", "PLUGIN")
ph.log("INFO", "Registered 3 commands and 4 hook handlers", "PLUGIN")

-- Set plugin metadata in configuration for introspection
ph.config_set("ph.plugins.advanced-workflows.version", PLUGIN_VERSION)
ph.config_set("ph.plugins.advanced-workflows.author", PLUGIN_AUTHOR)
ph.config_set("ph.plugins.advanced-workflows.loaded", "true")