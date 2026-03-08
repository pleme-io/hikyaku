use futures::StreamExt;
use image::DynamicImage;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;

use crate::config::GraphicsProtocol;

/// Terminal graphics renderer.
///
/// Detects the best available graphics protocol (Kitty, Sixel, Halfblocks)
/// and renders images inline. On Ghostty, Kitty protocol is used for GPU-
/// accelerated image display.
pub struct ImageRenderer {
    picker: Picker,
}

impl ImageRenderer {
    pub fn new(protocol: &GraphicsProtocol) -> Self {
        let mut picker =
            Picker::from_query_stdio().unwrap_or_else(|_| Picker::from_fontsize((8, 16)));

        match protocol {
            GraphicsProtocol::Auto => {}
            GraphicsProtocol::Kitty => picker.set_protocol_type(ProtocolType::Kitty),
            GraphicsProtocol::Sixel => picker.set_protocol_type(ProtocolType::Sixel),
            GraphicsProtocol::Halfblocks => picker.set_protocol_type(ProtocolType::Halfblocks),
        }

        Self { picker }
    }

    pub fn prepare_image(&self, image: DynamicImage) -> StatefulProtocol {
        self.picker.new_resize_protocol(image)
    }

    /// Render HTML to a PNG image using headless Chromium via chromiumoxide.
    ///
    /// This is the core GPU-rendering pipeline:
    /// 1. Launch headless Chrome (or connect to existing)
    /// 2. Load HTML into a page
    /// 3. Screenshot the page as PNG
    /// 4. Return as DynamicImage for Kitty protocol display
    pub async fn render_html(
        html: &str,
        width: u32,
        height: u32,
    ) -> anyhow::Result<DynamicImage> {
        use chromiumoxide::browser::{Browser, BrowserConfig};

        let config = BrowserConfig::builder()
            .no_sandbox()
            .window_size(width, height)
            .build()
            .map_err(|e| anyhow::anyhow!("browser config error: {e}"))?;

        let (browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| anyhow::anyhow!("failed to launch browser: {e}"))?;

        // Spawn the browser event handler
        let handle = tokio::spawn(async move {
            while let Some(_event) = handler.next().await {}
        });

        let page = browser.new_page("about:blank").await?;

        // Set the HTML content
        page.set_content(html).await?;

        // Wait for rendering to settle
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Take screenshot
        let png_data = page.screenshot(
            chromiumoxide::page::ScreenshotParams::builder()
                .full_page(true)
                .build(),
        ).await?;

        // Clean up
        drop(page);
        drop(browser);
        handle.abort();

        let img = image::load_from_memory_with_format(&png_data, image::ImageFormat::Png)?;
        Ok(img)
    }

    /// Fallback: convert HTML to plain text.
    pub fn html_to_plain_text(html: &str) -> String {
        let mut result = String::with_capacity(html.len());
        let mut in_tag = false;

        for c in html.chars() {
            match c {
                '<' => in_tag = true,
                '>' => {
                    in_tag = false;
                    continue;
                }
                _ if in_tag => continue,
                _ => result.push(c),
            }
        }

        result = result.replace("&amp;", "&");
        result = result.replace("&lt;", "<");
        result = result.replace("&gt;", ">");
        result = result.replace("&quot;", "\"");
        result = result.replace("&nbsp;", " ");
        result = result.replace("&#39;", "'");

        let lines: Vec<&str> = result
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        lines.join("\n")
    }

    pub fn load_image_from_bytes(data: &[u8]) -> anyhow::Result<DynamicImage> {
        Ok(image::load_from_memory(data)?)
    }
}
