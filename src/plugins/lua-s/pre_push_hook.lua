-- Copyright (C) 2025 Pedro Henrique / phkaiser13
-- pre_push_hook.lua - Example plugin for a pre-push hook.
--
-- This script demonstrates the event-driven capabilities of the scripting
-- engine. It defines a global function, `on_pre_push`, which the ph
-- core will automatically call before executing a push operation.
--
-- The script can perform checks and return `false` to cancel the push,
-- allowing teams to enforce policies (e.g., linting, commit message
-- format) before code reaches the remote.
--
-- SPDX-License-Identifier: Apache-2.0

ph.log("INFO", "pre_push_hook.lua plugin loaded.")

---
-- Global hook function called by the ph core before a push operation.
-- @param remote (string)  The name of the remote being pushed to.
-- @param branch (string)  The name of the branch being pushed.
-- @return (boolean)       true to allow the push, false to cancel it.
---
function on_pre_push(remote, branch)
    ph.log("INFO", "Executing on_pre_push hook for remote '" .. remote .. "' and branch '" .. branch .. "'.")

    -- Policy Check: Prevent direct pushes to the 'main' branch.
    if branch == "main" then
        ph.log("ERROR", "Direct push to 'main' branch is forbidden by pre_push_hook.lua policy.")
        print("[POLICY VIOLATION] You cannot push directly to the main branch. Please use a pull request.")
        return false -- Cancels the push operation.
    end

    -- Example Check: Run a linter before pushing.
    print("Running linter before push...")
    local lint_status = os.execute("npm run lint")

    -- os.execute returns different formats depending on Lua version and OS.
    -- Normalize result to detect failure (non-zero exit code).
    if type(lint_status) == "number" and lint_status ~= 0 then
        ph.log("ERROR", "Linter failed. Cancelling push.")
        return false
    elseif type(lint_status) == "boolean" and not lint_status then
        ph.log("ERROR", "Linter failed. Cancelling push.")
        return false
    end

    print("Linter passed successfully.")

    -- All checks passed, allow the push.
    return true
end
