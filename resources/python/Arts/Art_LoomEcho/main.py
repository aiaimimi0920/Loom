#!/usr/bin/env python3
"""Loom packaged Python Art smoke fixture."""

import sys


def main(args):
    text = args.get("text", "")
    return {
        "content": [
            {
                "type": "text",
                "text": f"python art saw {text}",
            }
        ],
        "pythonExecutable": sys.executable,
    }
