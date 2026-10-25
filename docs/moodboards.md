# Moodboards

Open **Project moodboards** in the workspace toolbar. Boards belong to the current project and persist across app restarts. Create or rename a board, then add PNG, JPEG, or WebP images using the file picker, drag-and-drop, or clipboard paste. Captions save automatically. Removing an image or board never deletes the original imported file.

Choose **Attach to task** to add a board to the current idle task's next message. Remove an attachment with its × button before sending. Up to eight boards can accompany one message. Failed sends restore the draft and attachments. If a board was deleted or the task moved projects, the request fails instead of silently losing its reference.

Agents can discover project boards with `pandacode_list_moodboards`, read paginated image IDs and captions with `pandacode_read_moodboard`, then see an image with `pandacode_read_moodboard_image`. The last tool returns actual image content through Codex's dynamic tool interface or the local MCP bridge used by the other providers. The selected provider/model must support images. These tools are read-only and cannot access another project's boards or arbitrary paths. Existing Codex sessions that predate these dynamic tools need a new task to acquire them.

Images are stored as app-owned copies in the local database. Thumbnails load lazily; tools receive a bounded image rendition instead of the full-resolution source. Limits: 24 boards per project, 64 images per board, 12 MiB per import, and 192 MiB total project image storage. Decoding also enforces dimension and allocation limits. Board metadata is paginated in groups of 16 for tools; the UI can browse the full board.
