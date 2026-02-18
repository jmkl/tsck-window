#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone)]
pub struct SplitNode {
    pub window_index: usize,
    pub ratio_index: usize,
    pub is_horizontal: bool,
    pub parent_bounds: Rect,
}

pub fn calculate_fibonacci_layout_with_ratios(
    w: i32,
    h: i32,
    count: usize,
    swap: bool,
    ratios: &[f32],
) -> (Vec<Rect>, Vec<SplitNode>) {
    let mut rects = Vec::new();
    let mut nodes = Vec::new();

    if count == 0 {
        return (rects, nodes);
    }

    if count == 1 {
        rects.push(Rect {
            x: 0,
            y: 0,
            width: w,
            height: h,
        });
        nodes.push(SplitNode {
            window_index: 0,
            ratio_index: 0,
            is_horizontal: false,
            parent_bounds: Rect {
                x: 0,
                y: 0,
                width: w,
                height: h,
            },
        });
        return (rects, nodes);
    }

    // Main window uses first ratio (default 0.5 = 50%)
    let main_ratio = ratios.get(0).copied().unwrap_or(0.5).clamp(0.2, 0.8);
    let main_width = (w as f32 * main_ratio) as i32;
    let main_x = if swap { w - main_width } else { 0 };

    rects.push(Rect {
        x: main_x,
        y: 0,
        width: main_width,
        height: h,
    });

    nodes.push(SplitNode {
        window_index: 0,
        ratio_index: 0,
        is_horizontal: false,
        parent_bounds: Rect {
            x: 0,
            y: 0,
            width: w,
            height: h,
        },
    });

    if count == 2 {
        let secondary_x = if swap { 0 } else { main_width };
        rects.push(Rect {
            x: secondary_x,
            y: 0,
            width: w - main_width,
            height: h,
        });
        nodes.push(SplitNode {
            window_index: 1,
            ratio_index: 0, // Window 1 shares ratio 0 with window 0
            is_horizontal: false,
            parent_bounds: Rect {
                x: 0,
                y: 0,
                width: w,
                height: h,
            },
        });
        return (rects, nodes);
    }

    // Stack area
    let stack_x = if swap { 0 } else { main_width };
    let stack_width = w - main_width;

    // Recursively subdivide with ratios
    subdivide_fibonacci_with_ratios_tracked(
        &mut rects,
        &mut nodes,
        stack_x,
        0,
        stack_width,
        h,
        count - 1,
        0,
        swap,
        ratios,
    );

    (rects, nodes)
}

fn subdivide_fibonacci_with_ratios_tracked(
    rects: &mut Vec<Rect>,
    nodes: &mut Vec<SplitNode>,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    remaining: usize,
    depth: usize,
    swap: bool,
    ratios: &[f32],
) {
    if remaining == 0 {
        return;
    }

    if remaining == 1 {
        rects.push(Rect {
            x,
            y,
            width,
            height,
        });
        nodes.push(SplitNode {
            window_index: rects.len() - 1,
            ratio_index: rects.len() - 1,
            is_horizontal: depth % 2 == 0,
            parent_bounds: Rect {
                x,
                y,
                width,
                height,
            },
        });
        return;
    }

    let split_horizontal = depth % 2 == 0;
    let ratio_index = rects.len();
    let ratio = ratios
        .get(ratio_index)
        .copied()
        .unwrap_or(0.5)
        .clamp(0.2, 0.8);

    let parent_bounds = Rect {
        x,
        y,
        width,
        height,
    };

    if split_horizontal {
        // Split horizontally (top/bottom)
        let first_height = (height as f32 * ratio) as i32;
        let second_height = height - first_height;

        // First window (top)
        rects.push(Rect {
            x,
            y,
            width,
            height: first_height,
        });

        nodes.push(SplitNode {
            window_index: rects.len() - 1,
            ratio_index,
            is_horizontal: true,
            parent_bounds,
        });

        // Recurse for bottom
        subdivide_fibonacci_with_ratios_tracked(
            rects,
            nodes,
            x,
            y + first_height,
            width,
            second_height,
            remaining - 1,
            depth + 1,
            swap,
            ratios,
        );
    } else {
        // Split vertically (left/right)
        let first_width = (width as f32 * ratio) as i32;
        let second_width = width - first_width;

        let (first_x, first_w, second_x, second_w) = if swap {
            (x + first_width, second_width, x, first_width)
        } else {
            (x, first_width, x + first_width, second_width)
        };

        // First window
        rects.push(Rect {
            x: first_x,
            y,
            width: first_w,
            height,
        });

        nodes.push(SplitNode {
            window_index: rects.len() - 1,
            ratio_index,
            is_horizontal: false,
            parent_bounds,
        });

        // Recurse for second part
        subdivide_fibonacci_with_ratios_tracked(
            rects,
            nodes,
            second_x,
            y,
            second_w,
            height,
            remaining - 1,
            depth + 1,
            swap,
            ratios,
        );
    }
}

fn subdivide_fibonacci_with_ratios(
    rects: &mut Vec<Rect>,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    remaining: usize,
    depth: usize,
    swap: bool,
    ratios: &[f32],
    ratio_offset: usize,
) {
    if remaining == 0 {
        return;
    }

    if remaining == 1 {
        rects.push(Rect {
            x,
            y,
            width,
            height,
        });
        return;
    }

    let split_horizontal = depth % 2 == 0;

    // Get ratio for current split, using the correct offset
    let current_ratio_index = rects.len() - 1; // Number of rects already placed
    let ratio = ratios
        .get(current_ratio_index.saturating_sub(ratio_offset))
        .copied()
        .unwrap_or(0.5)
        .clamp(0.2, 0.8);

    // Safely get remaining ratios for recursion
    let next_ratios = ratios;

    if split_horizontal {
        // Split horizontally (top/bottom)
        let first_height = (height as f32 * ratio) as i32;
        let second_height = height - first_height;

        // First window (top)
        rects.push(Rect {
            x,
            y,
            width,
            height: first_height,
        });

        // Recurse for bottom
        subdivide_fibonacci_with_ratios(
            rects,
            x,
            y + first_height,
            width,
            second_height,
            remaining - 1,
            depth + 1,
            swap,
            next_ratios,
            ratio_offset,
        );
    } else {
        // Split vertically (left/right)
        let first_width = (width as f32 * ratio) as i32;
        let second_width = width - first_width;

        let (first_x, first_w, second_x, second_w) = if swap {
            (x + first_width, second_width, x, first_width)
        } else {
            (x, first_width, x + first_width, second_width)
        };

        // First window
        rects.push(Rect {
            x: first_x,
            y,
            width: first_w,
            height,
        });

        // Recurse for second part
        subdivide_fibonacci_with_ratios(
            rects,
            second_x,
            y,
            second_w,
            height,
            remaining - 1,
            depth + 1,
            swap,
            next_ratios,
            ratio_offset,
        );
    }
}
