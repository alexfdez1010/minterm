# Terminal assistant interactions

Minterm borrows explicit agent entry from Warp and an editable input surface from iTerm2. The mode switch keeps a shell and an interactive CLI in the same tab. The assistant can explain a selection or recent output and turn the answer into a draft task for that tab. [Warp agent mode](https://docs.warp.dev/quickstart/) · [iTerm2 Composer](https://iterm2.com/documentation-menu-items.html)

## Proposed next changes

| When | Action | What to verify |
| --- | --- | --- |
| A command exits with an error | Offer **Explain failure** on that command. Use shell integration to identify the command, exit status and corresponding output. Let the user edit the excerpt before sending it. | Run two failing commands in succession. Each explanation should quote only the chosen command’s output. |
| A development server cannot bind its port | Offer **Find listener** from the error. Open Ports at the conflicting port and show the local process or Docker mapping. | A bind error on a Docker-published port should lead to the matching container in one action, without calling a model. |
