# Window names

Windows of the same application often share a title that says little about the
work they hold: three VS Code windows called `index.ts`, two terminals called
`zsh`. A window name replaces that title in the switcher.

The switcher footer says it too: **Type > to name**.

1. Open the switcher with **Command + Tab** (or search with **Control + I**) and
   select the window you want to name.
2. Type `>` followed by the name, for example `>模型 1 · 训练监控`, and press
   **Enter**. The row now shows that name, and the name is matched by search:
   typing `模型` finds the window.
3. To go back to the real title, select the window and enter `>` with nothing
   after it.

Names are stored by application and window title, with the WindowServer identity
as a tie-breaker, so two windows that share a title keep their own names. A name
survives a restart and an application that rebuilds its accessibility objects.
They live in their own preference value next to the settings, are written on a
normal quit, and are validated when read back: a name that is too long, empty,
duplicated, or attached to an unknown application is dropped.

Names are plain text, up to 60 characters, with at most 200 windows named.
