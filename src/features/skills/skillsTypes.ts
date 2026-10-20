/** Starter documents and small helpers for the Skills and Scripts tabs. */

/** A blank skill: valid frontmatter plus one example parameter. */
export const NEW_SKILL_TEMPLATE = `---
name: New Skill
description: What this skill does.
category: general
parameters:
  - name: text
    description: The main input.
    required: true
---
Describe what to do with {{text}}.
`;

/** A blank script: valid metadata docstring plus a minimal, offline body. */
export const NEW_SCRIPT_TEMPLATE = `"""
name: New Script
description: What this script does.
category: general
parameters:
  - name: text
    description: The main input.
    required: true
allow_network: false
"""

import json
import os

params = json.load(open(os.environ["ENDLESSLI_INPUT"], encoding="utf-8"))
print(params.get("text", ""))
`;

/** Default sandbox timeout offered in the Scripts run panel. */
export const DEFAULT_SCRIPT_TIMEOUT_SECS = 60;
