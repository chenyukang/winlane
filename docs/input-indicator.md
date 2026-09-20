# Input source indicator

Open **Settings → Input** and enable **Always show the current input source**. The indicator follows the system input source while you use any app, including when Winlane's search and settings windows are closed.

- **Color bar** draws a strip at a screen edge. Choose a small, normal, or large thickness and a length of 25%, 50%, or 100% of that edge.
- **Name badge** shows the system's input-source name in a rounded color badge. Its text uses black or white for contrast.
- **Circle** draws a solid dot. Set **Width / diameter** to its diameter in screen points (pt); the height is kept equal automatically.
- **Rounded rectangle** draws a solid rounded shape with separate width and height. Shape dimensions accept whole numbers from 4 to 512 pt. The Small/Normal/Large size control applies only to bars and name badges.
- **Position** offers the four edge centers and four corners. Corner bars run horizontally; a full-width bar looks the same at either corner of that edge.
- **X offset / Y offset** adjust any style from the chosen position. Positive X moves right; positive Y moves down. Negative values move left or up. Values use screen points and apply separately to each selected display, not to the combined desktop. Badges, circles, and rounded rectangles start inside the usable area to avoid the menu bar, Dock, and notch; offsets can move them anywhere within the full screen. Placement is clamped to keep the whole indicator on screen.
- **Displays** chooses all connected displays or the main display. Placement updates when displays or Spaces change. The indicator is configured to remain visible in full-screen Spaces.
- **Input source settings** lists enabled keyboard layouts and input methods. Choose one, then click the color control. **Reset color** restores that source's default without affecting other colors. Turn off **Show the indicator for this input source** to hide any indicator style whenever that source is active. For example, hide ABC while keeping the indicator for Chinese input. Its saved color is kept for re-enabling later, and other sources are unaffected.

For a small dot, choose **Circle**, set the diameter to **10 pt**, select a corner, then adjust X and Y. Numeric fields save and update the indicator when you press Return or finish editing. Invalid or incomplete values keep the last saved settings active. The settings page scrolls to keep all controls accessible.

Default colors distinguish English (blue), Chinese (orange), Japanese (purple), Korean (green), and other sources (teal). Custom colors are stored by input-source ID, so changing its display name does not lose the color. The indicator starts disabled; its settings persist across restarts.

The indicator is display-only: clicks pass through, it never becomes the key window, and it does not select an input source. Winlane's default input preference is configured separately on the same page. It uses macOS input-source notifications instead of a polling timer. If an input method changes an internal mode without reporting a different source to macOS, the indicator continues to show the source reported by the system.
