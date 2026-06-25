# Deep Index Board

English | [简体中文](README.zh-CN.md)

Deep Index Board is a clipboard history tool for everyday use. It records copied content in the background and provides quick access, preview, search, favorites, text editing, and paste-again workflows.

## Features

### Clipboard History

- Automatically records copied text, images, files, and folders
- Opens the clipboard panel with a global shortcut, then pastes by clicking a history item
- Supports deleting individual history items
- Supports clearing non-favorited history items and removing images saved by the app

### Favorites

- Supports adding history items to favorites
- Keeps the favorites section fixed above the regular history list and collapsed by default
- Supports manually expanding/collapsing the favorites section and resizing it by dragging the divider
- Supports unfavoriting all items at once

### Search and Preview

- Supports keyword search across history
- Supports semantic search for images using natural-language descriptions
- Saves image content automatically and supports image preview
- Supports OCR text extraction for images and image files
- Shows content snapshots for text files

### Text Editing

- Text items can explicitly enter edit mode in the right-side preview pane
- Edited content must be manually saved as a new item or used to overwrite the current item
- Draft edits do not automatically modify the original history item

### App Experience

- Supports resizing the window and dragging the window position
- Supports toggling launch at startup
- Shows current memory usage in the status bar

## Usage

After launch, the app listens to clipboard changes in the background.

- Windows: press `Alt + V` to open the panel
- macOS: press `Control + V` to open the panel

In the panel, you can search, preview, favorite, edit, delete, and select clipboard history items. The top-level clear action removes non-favorited history items while keeping the favorites section intact.

## License

MIT License. You may use, modify, and distribute this project freely, as long as the copyright and license notice are preserved.
