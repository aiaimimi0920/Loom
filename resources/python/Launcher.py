#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Loom Python Plugin Launcher
==============================

This is the core launcher script for running Python Arts (plugins).
It manages the Python environment, dynamically loads plugins, and handles
execution with proper error handling.

Architecture:
- Layer 1: Common libraries in bin/python-embed/site-packages (numpy, cv2, pillow, etc.)
- Layer 2: Plugin-specific libraries in Arts/{plugin}/site-packages (optional overrides)
- Plugin code: Arts/{plugin}/main.py

Usage:
    python Launcher.py <json_request>

    OR via stdin:
    echo '{"request_id": "...", ...}' | python Launcher.py

Request Format (JSON):
{
    "request_id": "uuid",
    "art_id": "art_color_transfer",
    "plugin_path": "C:/path/to/Art_ColorTransfer",
    "params": {
        "input_path": "...",
        "output_path": "...",
        ...
    }
}

Response Format (JSON to stdout):
{
    "request_id": "uuid",
    "status": 200,
    "data": { ... }
}

Error Response:
{
    "request_id": "uuid",
    "status": 500,
    "error": {
        "code": "PLUGIN_ERROR",
        "message": "...",
        "traceback": "..."
    }
}
"""

import sys
import os
import json
import importlib
import importlib.util
import traceback
import time
from pathlib import Path
from typing import Any, Dict, Optional, List


# =============================================================================
# Configuration
# =============================================================================

# Stable process-runtime status codes.
STATUS_SUCCESS = 200
STATUS_BAD_REQUEST = 400
STATUS_NOT_FOUND = 404
STATUS_PLUGIN_ERROR = 500
STATUS_ENGINE_ERROR = 502


# =============================================================================
# Path Management
# =============================================================================

class PathManager:
    """
    Manages sys.path for layered dependency resolution.

    Priority order (highest to lowest):
    1. Plugin's site-packages (if exists) - for private/override dependencies
    2. Common site-packages (bin/python-embed/site-packages)
    3. Standard Python paths
    """

    def __init__(self, base_dir: Path):
        """
        Initialize PathManager.

        Args:
            base_dir: The Loom base directory (parent of bin/, python/)
        """
        self.base_dir = base_dir.resolve()
        self.common_site_packages = self.base_dir / "bin" / "python-embed" / "site-packages"
        self._original_path: List[str] = []
        self._added_paths: List[str] = []
        self._loaded_modules: List[str] = []

    def setup_common_paths(self) -> None:
        """Ensure common site-packages is in sys.path."""
        common_path = str(self.common_site_packages)
        if common_path not in sys.path and self.common_site_packages.exists():
            sys.path.append(common_path)
            self._added_paths.append(common_path)

    def inject_plugin_paths(self, plugin_dir: Path) -> None:
        """
        Inject plugin-specific paths into sys.path.

        Plugin's site-packages is inserted at index 0 to override common libraries.
        Plugin directory is also added to allow relative imports.

        Args:
            plugin_dir: Path to the plugin directory
        """
        # Save original path for cleanup
        self._original_path = sys.path.copy()

        plugin_path = str(plugin_dir.resolve())
        plugin_site_packages = plugin_dir / "site-packages"

        # Add plugin directory itself
        if plugin_path not in sys.path:
            sys.path.insert(0, plugin_path)
            self._added_paths.append(plugin_path)

        # Add plugin's site-packages at highest priority (index 0)
        if plugin_site_packages.exists():
            plugin_sp_path = str(plugin_site_packages.resolve())
            if plugin_sp_path not in sys.path:
                sys.path.insert(0, plugin_sp_path)
                self._added_paths.append(plugin_sp_path)

    def cleanup(self) -> None:
        """
        Clean up sys.path and unload plugin modules.

        This prevents environment pollution between plugin executions.
        """
        # Remove added paths
        for path in self._added_paths:
            if path in sys.path:
                sys.path.remove(path)

        # Unload plugin modules (those loaded after injection)
        for mod_name in list(sys.modules.keys()):
            if mod_name in self._loaded_modules:
                del sys.modules[mod_name]

        self._added_paths.clear()
        self._loaded_modules.clear()

    def track_module(self, module_name: str) -> None:
        """Track a loaded module for later cleanup."""
        self._loaded_modules.append(module_name)


# =============================================================================
# Plugin Loader
# =============================================================================

class PluginLoader:
    """
    Dynamically loads and executes Python plugins.
    """

    def __init__(self, path_manager: PathManager):
        self.path_manager = path_manager

    def load_plugin(self, plugin_dir: Path) -> Any:
        """
        Load a plugin's main module.

        Args:
            plugin_dir: Path to the plugin directory

        Returns:
            The loaded module object

        Raises:
            FileNotFoundError: If main.py doesn't exist
            ImportError: If module loading fails
        """
        main_file = plugin_dir / "main.py"

        if not main_file.exists():
            raise FileNotFoundError(f"Plugin entry point not found: {main_file}")

        # Create a unique module name to avoid conflicts
        module_name = f"art_plugin_{plugin_dir.name}_{int(time.time() * 1000)}"

        # Load the module using importlib
        spec = importlib.util.spec_from_file_location(module_name, main_file)
        if spec is None or spec.loader is None:
            raise ImportError(f"Cannot load module spec from {main_file}")

        module = importlib.util.module_from_spec(spec)
        sys.modules[module_name] = module
        self.path_manager.track_module(module_name)

        try:
            spec.loader.exec_module(module)
        except Exception as e:
            # Clean up on failure
            if module_name in sys.modules:
                del sys.modules[module_name]
            raise ImportError(f"Failed to execute module: {e}") from e

        return module

    def execute(self, module: Any, params: Dict[str, Any]) -> Dict[str, Any]:
        """
        Execute the plugin's entry point function.

        The plugin must define either:
        - main(args: dict) -> dict
        - entry_point(args: dict) -> dict

        Args:
            module: The loaded plugin module
            params: Parameters to pass to the plugin

        Returns:
            Result dictionary from the plugin

        Raises:
            AttributeError: If no entry point is found
            Exception: Any error from plugin execution
        """
        # Look for entry point function
        entry_func = None
        for func_name in ['main', 'entry_point', 'run']:
            if hasattr(module, func_name):
                entry_func = getattr(module, func_name)
                break

        if entry_func is None:
            raise AttributeError(
                f"Plugin must define one of: main(), entry_point(), or run()"
            )

        # Execute the plugin
        result = entry_func(params)

        # Ensure result is a dict
        if not isinstance(result, dict):
            result = {"result": result}

        return result


# =============================================================================
# Request Handler
# =============================================================================

def create_response(
    request_id: str,
    status: int,
    data: Optional[Dict[str, Any]] = None,
    error: Optional[Dict[str, Any]] = None
) -> Dict[str, Any]:
    """Create a standardized response dictionary."""
    response = {
        "request_id": request_id,
        "status": status,
    }
    if data is not None:
        response["data"] = data
    if error is not None:
        response["error"] = error
    return response


def handle_request(request: Dict[str, Any], base_dir: Path) -> Dict[str, Any]:
    """
    Handle a single plugin execution request.

    Args:
        request: The parsed request dictionary
        base_dir: Loom base directory

    Returns:
        Response dictionary
    """
    request_id = request.get("request_id", "unknown")

    try:
        # Validate required fields
        if "plugin_path" not in request:
            return create_response(
                request_id, STATUS_BAD_REQUEST,
                error={
                    "code": "MISSING_FIELD",
                    "message": "Missing required field: plugin_path"
                }
            )

        plugin_path = Path(request["plugin_path"])
        if not plugin_path.exists():
            return create_response(
                request_id, STATUS_NOT_FOUND,
                error={
                    "code": "PLUGIN_NOT_FOUND",
                    "message": f"Plugin directory not found: {plugin_path}"
                }
            )

        params = request.get("params", {})

        # Initialize path manager and setup environment
        path_manager = PathManager(base_dir)
        path_manager.setup_common_paths()
        path_manager.inject_plugin_paths(plugin_path)

        try:
            # Load and execute plugin
            loader = PluginLoader(path_manager)
            start_time = time.perf_counter()

            module = loader.load_plugin(plugin_path)
            result = loader.execute(module, params)

            elapsed_ms = int((time.perf_counter() - start_time) * 1000)

            # Add processing time to result
            result["processing_time_ms"] = elapsed_ms

            return create_response(request_id, STATUS_SUCCESS, data=result)

        finally:
            # Always cleanup paths
            path_manager.cleanup()

    except FileNotFoundError as e:
        return create_response(
            request_id, STATUS_NOT_FOUND,
            error={
                "code": "FILE_NOT_FOUND",
                "message": str(e)
            }
        )

    except (ImportError, AttributeError) as e:
        return create_response(
            request_id, STATUS_PLUGIN_ERROR,
            error={
                "code": "PLUGIN_LOAD_ERROR",
                "message": str(e),
                "traceback": traceback.format_exc()
            }
        )

    except Exception as e:
        return create_response(
            request_id, STATUS_PLUGIN_ERROR,
            error={
                "code": "PLUGIN_ERROR",
                "message": str(e),
                "traceback": traceback.format_exc()
            }
        )


# =============================================================================
# Main Entry Point
# =============================================================================

def main():
    """Main entry point for the launcher."""
    # Determine base directory (parent of python/)
    script_dir = Path(__file__).parent.resolve()
    base_dir = script_dir.parent  # Loom root

    # Read request from command line argument or stdin
    request_json = None

    if len(sys.argv) > 1:
        # Request passed as command line argument
        request_json = sys.argv[1]
    else:
        # Try to read from stdin
        if not sys.stdin.isatty():
            request_json = sys.stdin.read().strip()

    if not request_json:
        # No input - output usage
        response = create_response(
            "none", STATUS_BAD_REQUEST,
            error={
                "code": "NO_INPUT",
                "message": "No request provided. Pass JSON as argument or via stdin."
            }
        )
        print(json.dumps(response, ensure_ascii=False))
        sys.exit(1)

    # Parse request
    try:
        request = json.loads(request_json)
    except json.JSONDecodeError as e:
        response = create_response(
            "none", STATUS_BAD_REQUEST,
            error={
                "code": "INVALID_JSON",
                "message": f"Failed to parse JSON: {e}"
            }
        )
        print(json.dumps(response, ensure_ascii=False))
        sys.exit(1)

    # Handle request
    response = handle_request(request, base_dir)

    # Output response as JSON to stdout
    print(json.dumps(response, ensure_ascii=False))

    # Exit with appropriate code
    sys.exit(0 if response["status"] == STATUS_SUCCESS else 1)


if __name__ == "__main__":
    main()
