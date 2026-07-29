#[cfg(test)]
mod tests {
    use super::{
        add_profile_layout, profile_manager_layout, scale_logical, DialogLayoutInput,
        DialogPalette, DialogTheme,
    };
    use crate::windows::ProfileUsageStatus;

    #[test]
    fn profile_status_colors_match_design_md() {
        let light = DialogPalette::for_theme(DialogTheme::Light);
        assert_eq!(light.status(ProfileUsageStatus::Healthy), 0x0074_c748);
        assert_eq!(light.status(ProfileUsageStatus::Warning), 0x0023_a6f5);
        assert_eq!(light.status(ProfileUsageStatus::Critical), 0x005c_5cff);
    }

    #[test]
    fn logical_dimensions_scale_at_supported_dpis() {
        for (dpi, expected_row_height) in [(96, 56), (120, 70), (144, 84), (168, 98), (192, 112)] {
            assert_eq!(scale_logical(56, dpi), expected_row_height);
        }
    }

    #[test]
    fn manager_layout_keeps_buttons_single_line_and_uses_whole_button_fallback() {
        let one_row =
            profile_manager_layout(DialogLayoutInput::new(620, 96, false, [74, 70, 82, 62]));
        assert_eq!(one_row.action_rows, 1);
        assert!(one_row
            .action_buttons
            .windows(2)
            .all(|pair| pair[0].right <= pair[1].left));

        let two_rows =
            profile_manager_layout(DialogLayoutInput::new(420, 96, false, [150, 146, 158, 138]));
        assert_eq!(two_rows.action_rows, 2);
        assert!(two_rows
            .action_buttons
            .iter()
            .all(|rect| rect.width() >= 32));
    }

    #[test]
    fn arabic_layout_mirrors_selection_edge_and_action_alignment() {
        let ltr = profile_manager_layout(DialogLayoutInput::new(620, 96, false, [74, 70, 82, 62]));
        let rtl = profile_manager_layout(DialogLayoutInput::new(620, 96, true, [74, 70, 82, 62]));
        assert_eq!(ltr.selection_edge.left, ltr.list.left);
        assert_eq!(rtl.selection_edge.right, rtl.list.right);
        assert_eq!(ltr.action_buttons[3].right, ltr.content.right);
        assert_eq!(rtl.action_buttons[0].left, rtl.content.left);
    }

    #[test]
    fn dialog_layouts_stay_inside_client_and_keep_controls_separate_at_supported_dpis() {
        for dpi in [96, 120, 144, 168, 192] {
            let input = DialogLayoutInput::new(620, dpi, false, [74, 70, 82, 62]);
            let manager = profile_manager_layout(input);
            assert_rectangles_are_valid(
                manager.client,
                &[
                    manager.list,
                    manager.add_control,
                    manager.name_label,
                    manager.name_edit,
                    manager.action_buttons[0],
                    manager.action_buttons[1],
                    manager.action_buttons[2],
                    manager.action_buttons[3],
                ],
            );

            let add = add_profile_layout(input);
            assert_rectangles_are_valid(
                add.client,
                &[
                    add.name_label,
                    add.name_edit,
                    add.action_buttons[0],
                    add.action_buttons[1],
                ],
            );
        }
    }

    fn assert_rectangles_are_valid(client: super::LogicalRect, controls: &[super::LogicalRect]) {
        assert!(controls.iter().all(|rect| rect.is_inside(client)));
        assert!(controls
            .iter()
            .enumerate()
            .all(|(index, rect)| controls[index + 1..]
                .iter()
                .all(|other| !rect.intersects(*other))));
    }
}
use crate::windows::ProfileUsageStatus;

/// 대화 상자의 밝기 테마입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogTheme {
    /// 밝은 Windows 표면에 맞춘 테마입니다.
    Light,
    /// 어두운 Windows 표면에 맞춘 테마입니다.
    Dark,
}

/// Win32 `COLORREF` 순서(BB-GG-RR)와 불투명도를 함께 보관하는 대화 상자 색상 토큰입니다.
///
/// 반투명 토큰은 `colorref`를 기본 색으로, `opacity`를 0부터 255까지의 알파 값으로
/// 전달합니다. 실제 합성은 그리기 계층에서 현재 표면과 함께 수행해야 합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DialogColor {
    /// Win32 GDI에 전달할 BB-GG-RR 순서의 색상 값입니다.
    pub colorref: u32,
    /// 기본 색상을 표면에 합성할 때 사용할 0부터 255까지의 불투명도입니다.
    pub opacity: u8,
}

impl DialogColor {
    /// 완전히 불투명한 Win32 색상 토큰을 만듭니다.
    pub const fn opaque(colorref: u32) -> Self {
        Self {
            colorref,
            opacity: u8::MAX,
        }
    }

    /// 지정한 불투명도의 Win32 색상 토큰을 만듭니다.
    pub const fn translucent(colorref: u32, opacity: u8) -> Self {
        Self { colorref, opacity }
    }
}

/// 대화 상자 렌더링에 공통으로 사용하는 색상 토큰 모음입니다.
///
/// 모든 색상은 Win32 `COLORREF` 바이트 순서이며, 반투명 토큰의 실제 합성은 호출자가
/// 현재 표면에 대해 수행합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DialogPalette {
    /// 창의 기본 배경색입니다.
    pub background: DialogColor,
    /// 일반 콘텐츠 표면색입니다.
    pub surface: DialogColor,
    /// 선택 행 등 한 단계 올라온 표면색입니다.
    pub elevated_surface: DialogColor,
    /// 표준 테두리색입니다.
    pub border: DialogColor,
    /// 약한 구분선에 사용하는 반투명 테두리색입니다.
    pub subtle_border: DialogColor,
    /// 기본 텍스트색입니다.
    pub text: DialogColor,
    /// 보조 텍스트색입니다.
    pub secondary_text: DialogColor,
    /// 비활성 또는 보조 정보 텍스트색입니다.
    pub muted_text: DialogColor,
    /// 진행 표시줄의 배경 트랙색입니다.
    pub progress_track: DialogColor,
    /// 호버 표면 오버레이색입니다.
    pub hover: DialogColor,
    /// 눌린 표면 오버레이색입니다.
    pub pressed: DialogColor,
    /// 키보드 포커스 테두리색입니다.
    pub focus: DialogColor,
    /// 정상 사용량의 의미 색상입니다.
    pub healthy: DialogColor,
    /// 주의 사용량의 의미 색상입니다.
    pub warning: DialogColor,
    /// 위험 사용량의 의미 색상입니다.
    pub critical: DialogColor,
}

impl DialogPalette {
    /// 지정한 테마의 디자인 시스템 색상 토큰을 반환합니다.
    pub const fn for_theme(theme: DialogTheme) -> Self {
        match theme {
            DialogTheme::Light => Self {
                background: DialogColor::opaque(0x00f3_f3f3),
                surface: DialogColor::opaque(0x00ff_ffff),
                elevated_surface: DialogColor::opaque(0x00fa_fafa),
                border: DialogColor::opaque(0x00d5_d5d5),
                subtle_border: DialogColor::translucent(0x0000_0000, 20),
                text: DialogColor::opaque(0x0020_2020),
                secondary_text: DialogColor::opaque(0x0050_5050),
                muted_text: DialogColor::opaque(0x0073_7373),
                progress_track: DialogColor::translucent(0x0000_0000, 36),
                hover: DialogColor::translucent(0x0000_0000, 13),
                pressed: DialogColor::translucent(0x0000_0000, 23),
                focus: DialogColor::opaque(0x004d_8627),
                healthy: DialogColor::opaque(0x0074_c748),
                warning: DialogColor::opaque(0x0023_a6f5),
                critical: DialogColor::opaque(0x005c_5cff),
            },
            DialogTheme::Dark => Self {
                background: DialogColor::opaque(0x001f_1f1f),
                surface: DialogColor::opaque(0x0026_2626),
                elevated_surface: DialogColor::opaque(0x002c_2c2c),
                border: DialogColor::opaque(0x0040_4040),
                subtle_border: DialogColor::translucent(0x00ff_ffff, 20),
                text: DialogColor::opaque(0x00ee_eeee),
                secondary_text: DialogColor::opaque(0x00c8_c8c8),
                muted_text: DialogColor::opaque(0x0097_9797),
                progress_track: DialogColor::translucent(0x00ff_ffff, 36),
                hover: DialogColor::translucent(0x00ff_ffff, 15),
                pressed: DialogColor::translucent(0x00ff_ffff, 26),
                focus: DialogColor::opaque(0x0074_c748),
                healthy: DialogColor::opaque(0x0074_c748),
                warning: DialogColor::opaque(0x0023_a6f5),
                critical: DialogColor::opaque(0x005c_5cff),
            },
        }
    }

    /// 프로필 사용량 상태에 대응하는 불투명한 Win32 색상 값을 반환합니다.
    pub const fn status(self, status: ProfileUsageStatus) -> u32 {
        match status {
            ProfileUsageStatus::Healthy => self.healthy.colorref,
            ProfileUsageStatus::Warning => self.warning.colorref,
            ProfileUsageStatus::Critical => self.critical.colorref,
        }
    }
}

/// 대화 상자 외곽의 논리 픽셀 여백입니다.
pub const OUTER_PADDING: i32 = 16;
/// 프로필 목록 한 행의 논리 픽셀 높이입니다.
pub const ROW_HEIGHT: i32 = 56;
/// 버튼과 편집 컨트롤의 최소 논리 픽셀 높이입니다.
pub const CONTROL_HEIGHT: i32 = 32;
/// 선택된 프로필을 표시하는 논리 픽셀 가장자리 두께입니다.
pub const SELECTION_EDGE: i32 = 3;
/// 목록 행에 표시할 진행 표시줄의 논리 픽셀 높이입니다.
pub const PROGRESS_HEIGHT: i32 = 3;
/// 밀집한 컨트롤 사이의 논리 픽셀 간격입니다.
pub const GAP_6: i32 = 6;
/// 같은 행의 컨트롤 사이의 논리 픽셀 간격입니다.
pub const GAP_8: i32 = 8;
/// 섹션 사이의 논리 픽셀 간격입니다.
pub const GAP_12: i32 = 12;

const LABEL_HEIGHT: i32 = 18;
const PROFILE_LIST_ROWS: i32 = 3;

/// 논리 크기를 지정 DPI의 물리 픽셀 크기로 반올림해 변환합니다.
///
/// DPI가 96보다 작으면 96으로 처리해 비정상적인 축소를 방지합니다.
pub const fn scale_logical(value: i32, dpi: u32) -> i32 {
    let dpi = at_least_96_dpi(dpi);
    ((value as i64 * dpi as i64 + 48) / 96) as i32
}

/// 측정한 버튼 문자열 폭을 단일 행 버튼의 물리 픽셀 폭으로 변환합니다.
///
/// `measured_text_width`는 현재 DPI와 대화 상자 글꼴로 측정한 값이어야 하며, 반환값은
/// 텍스트 양쪽 여백과 최소 폭을 보장합니다. 텍스트는 이 함수에서 절단하지 않습니다.
pub const fn button_width(measured_text_width: i32, dpi: u32) -> i32 {
    let padded = measured_text_width + scale_logical(24, dpi);
    if padded > scale_logical(72, dpi) {
        padded
    } else {
        scale_logical(72, dpi)
    }
}

/// 물리 픽셀 좌표로 표현한 왼쪽·위·오른쪽·아래 사각형입니다.
///
/// 이름은 논리 레이아웃에서 유래했지만, 레이아웃 함수가 반환하는 좌표는 모두 입력 DPI에
/// 맞춰 변환된 물리 픽셀입니다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LogicalRect {
    /// 왼쪽 물리 픽셀 좌표입니다.
    pub left: i32,
    /// 위쪽 물리 픽셀 좌표입니다.
    pub top: i32,
    /// 오른쪽 물리 픽셀 좌표(배타적)입니다.
    pub right: i32,
    /// 아래쪽 물리 픽셀 좌표(배타적)입니다.
    pub bottom: i32,
}

impl LogicalRect {
    /// 두 모서리 좌표로 사각형을 만듭니다.
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// 사각형의 물리 픽셀 폭을 반환합니다.
    pub const fn width(self) -> i32 {
        self.right - self.left
    }

    /// 사각형의 물리 픽셀 높이를 반환합니다.
    pub const fn height(self) -> i32 {
        self.bottom - self.top
    }

    /// 이 사각형이 `container`의 물리 픽셀 경계를 벗어나지 않는지 반환합니다.
    pub const fn is_inside(self, container: Self) -> bool {
        self.left >= container.left
            && self.top >= container.top
            && self.right <= container.right
            && self.bottom <= container.bottom
            && self.width() >= 0
            && self.height() >= 0
    }

    /// 두 사각형의 내부가 겹치는지 반환합니다. 모서리만 맞닿으면 겹치지 않습니다.
    pub const fn intersects(self, other: Self) -> bool {
        self.left < other.right
            && self.right > other.left
            && self.top < other.bottom
            && self.bottom > other.top
    }
}

/// 순수 대화 상자 레이아웃 계산에 필요한 입력값입니다.
///
/// `client_width`는 96 DPI 기준 논리 폭이고, `action_text_widths`는 현재 DPI와 글꼴로
/// 측정한 네 관리자 버튼의 문자열 폭입니다. `rtl`은 아랍어처럼 오른쪽에서 왼쪽으로
/// 배치해야 하는 로캘을 호출자가 명시하는 값입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DialogLayoutInput {
    /// 요청한 대화 상자 클라이언트 논리 폭입니다.
    pub client_width: i32,
    /// 현재 창 DPI입니다.
    pub dpi: u32,
    /// 오른쪽에서 왼쪽으로 배치할지 여부입니다.
    pub rtl: bool,
    /// Rename, Login, Logout, Delete 순서의 측정된 물리 픽셀 문자열 폭입니다.
    pub action_text_widths: [i32; 4],
}

impl DialogLayoutInput {
    /// 측정된 관리자 버튼 폭과 방향을 포함한 레이아웃 입력을 만듭니다.
    ///
    /// 음수 폭은 허용하지 않으므로 0으로 보정합니다. 반환 레이아웃은 최소 버튼 폭과 최대
    /// 두 액션 행을 보장하기 위해 요청 폭보다 넓어질 수 있습니다.
    pub const fn new(client_width: i32, dpi: u32, rtl: bool, action_text_widths: [i32; 4]) -> Self {
        Self {
            client_width: non_negative(client_width),
            dpi,
            rtl,
            action_text_widths: [
                non_negative(action_text_widths[0]),
                non_negative(action_text_widths[1]),
                non_negative(action_text_widths[2]),
                non_negative(action_text_widths[3]),
            ],
        }
    }
}

const fn at_least_96_dpi(dpi: u32) -> u32 {
    if dpi < 96 {
        96
    } else {
        dpi
    }
}

const fn non_negative(value: i32) -> i32 {
    if value < 0 {
        0
    } else {
        value
    }
}

/// 프로필 관리자 대화 상자가 그릴 물리 픽셀 사각형 모음입니다.
///
/// `action_buttons`는 Rename, Login, Logout, Delete 순서를 유지합니다. 버튼 문자열은
/// 항상 한 줄이며, 한 행에 맞지 않으면 버튼 단위로만 두 번째 행으로 이동합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileManagerLayout {
    /// 최종 클라이언트 물리 픽셀 경계입니다.
    pub client: LogicalRect,
    /// 외곽 여백을 제외한 콘텐츠 물리 픽셀 경계입니다.
    pub content: LogicalRect,
    /// 프로필 목록 물리 픽셀 경계입니다.
    pub list: LogicalRect,
    /// 선택 상태를 그릴 목록의 시작 또는 끝 가장자리 사각형입니다.
    pub selection_edge: LogicalRect,
    /// 목록 바로 아래의 프로필 추가 컨트롤 물리 픽셀 경계입니다.
    pub add_control: LogicalRect,
    /// 프로필 이름 라벨의 물리 픽셀 경계입니다.
    pub name_label: LogicalRect,
    /// 프로필 이름 편집 컨트롤의 물리 픽셀 경계입니다.
    pub name_edit: LogicalRect,
    /// Rename, Login, Logout, Delete 버튼의 물리 픽셀 경계입니다.
    pub action_buttons: [LogicalRect; 4],
    /// 버튼을 수용하는 액션 행 수이며 1 또는 2입니다.
    pub action_rows: u8,
}

/// 프로필 추가 대화 상자가 그릴 물리 픽셀 사각형 모음입니다.
///
/// `action_buttons`는 Add, Cancel 순서이며, 문자열을 자르지 않기 위해 필요한 경우
/// 버튼 전체만 두 번째 행으로 이동합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddProfileLayout {
    /// 최종 클라이언트 물리 픽셀 경계입니다.
    pub client: LogicalRect,
    /// 외곽 여백을 제외한 콘텐츠 물리 픽셀 경계입니다.
    pub content: LogicalRect,
    /// 프로필 이름 라벨의 물리 픽셀 경계입니다.
    pub name_label: LogicalRect,
    /// 프로필 이름 편집 컨트롤의 물리 픽셀 경계입니다.
    pub name_edit: LogicalRect,
    /// Add, Cancel 버튼의 물리 픽셀 경계입니다.
    pub action_buttons: [LogicalRect; 2],
    /// 버튼을 수용하는 액션 행 수이며 1 또는 2입니다.
    pub action_rows: u8,
}

/// 프로필 관리자 대화 상자의 순수·결정적 물리 픽셀 레이아웃을 계산합니다.
///
/// 입력은 논리 클라이언트 폭, DPI, RTL 여부, 현재 글꼴로 측정한 액션 문자열 폭입니다.
/// 반환된 사각형은 물리 픽셀 좌표이고 모두 `client` 안에 있습니다. RTL이면 선택 막대와
/// 액션 정렬을 좌우 반전하며, 액션 문자열은 절단하거나 줄바꿈하지 않고 최대 두 행의 버튼
/// 단위 대체 배치만 사용합니다.
pub fn profile_manager_layout(input: DialogLayoutInput) -> ProfileManagerLayout {
    let scale = |value| scale_logical(value, input.dpi);
    let padding = scale(OUTER_PADDING);
    let gap_6 = scale(GAP_6);
    let gap_8 = scale(GAP_8);
    let gap_12 = scale(GAP_12);
    let control_height = scale(CONTROL_HEIGHT);
    let widths = input
        .action_text_widths
        .map(|width| button_width(width, input.dpi));
    let single_row_width = widths.iter().sum::<i32>() + gap_8 * 3;
    let two_row_width = (widths[0] + gap_8 + widths[1]).max(widths[2] + gap_8 + widths[3]);
    let requested_content_width = scale(input.client_width);
    let content_width = requested_content_width.max(two_row_width);
    let action_rows = if single_row_width <= content_width {
        1
    } else {
        2
    };
    let client_width = content_width + padding * 2;

    let list = LogicalRect::new(
        padding,
        padding,
        padding + content_width,
        padding + scale(ROW_HEIGHT * PROFILE_LIST_ROWS),
    );
    let selection_width = scale(SELECTION_EDGE);
    let selection_edge = if input.rtl {
        LogicalRect::new(
            list.right - selection_width,
            list.top,
            list.right,
            list.bottom,
        )
    } else {
        LogicalRect::new(
            list.left,
            list.top,
            list.left + selection_width,
            list.bottom,
        )
    };
    let add_top = list.bottom + gap_6;
    let add_left = if input.rtl {
        list.right - control_height
    } else {
        list.left
    };
    let add_control = LogicalRect::new(
        add_left,
        add_top,
        add_left + control_height,
        add_top + control_height,
    );
    let label_top = add_control.bottom + gap_12;
    let name_label = LogicalRect::new(
        list.left,
        label_top,
        list.right,
        label_top + scale(LABEL_HEIGHT),
    );
    let edit_top = name_label.bottom + gap_6;
    let name_edit = LogicalRect::new(list.left, edit_top, list.right, edit_top + control_height);
    let actions_top = name_edit.bottom + gap_12;
    let action_buttons = manager_action_buttons(
        list,
        actions_top,
        control_height,
        gap_8,
        widths,
        action_rows,
        input.rtl,
    );
    let actions_height =
        control_height * i32::from(action_rows) + gap_8 * i32::from(action_rows - 1);
    let client = LogicalRect::new(0, 0, client_width, actions_top + actions_height + padding);

    ProfileManagerLayout {
        client,
        content: LogicalRect::new(list.left, list.top, list.right, client.bottom - padding),
        list,
        selection_edge,
        add_control,
        name_label,
        name_edit,
        action_buttons,
        action_rows,
    }
}

/// 프로필 추가 대화 상자의 순수·결정적 물리 픽셀 레이아웃을 계산합니다.
///
/// `action_text_widths`의 앞 두 값은 각각 Add와 Cancel의 현재 글꼴 측정 폭입니다. 반환된
/// 사각형은 물리 픽셀 좌표이며, RTL에서 액션의 시작·끝 정렬을 반전하고 텍스트를 자르지
/// 않으며 최대 두 행의 버튼 단위 대체 배치만 사용합니다.
pub fn add_profile_layout(input: DialogLayoutInput) -> AddProfileLayout {
    let scale = |value| scale_logical(value, input.dpi);
    let padding = scale(OUTER_PADDING);
    let gap_6 = scale(GAP_6);
    let gap_8 = scale(GAP_8);
    let gap_12 = scale(GAP_12);
    let control_height = scale(CONTROL_HEIGHT);
    let widths = [
        button_width(input.action_text_widths[0], input.dpi),
        button_width(input.action_text_widths[1], input.dpi),
    ];
    let content_width = scale(input.client_width).max(widths[0].max(widths[1]));
    let action_rows = if widths[0] + gap_8 + widths[1] <= content_width {
        1
    } else {
        2
    };
    let client_width = content_width + padding * 2;
    let content = LogicalRect::new(padding, padding, padding + content_width, padding);
    let name_label = LogicalRect::new(
        content.left,
        content.top,
        content.right,
        content.top + scale(LABEL_HEIGHT),
    );
    let edit_top = name_label.bottom + gap_6;
    let name_edit = LogicalRect::new(
        content.left,
        edit_top,
        content.right,
        edit_top + control_height,
    );
    let actions_top = name_edit.bottom + gap_12;
    let action_buttons = add_action_buttons(
        content,
        actions_top,
        control_height,
        gap_8,
        widths,
        action_rows,
        input.rtl,
    );
    let actions_height =
        control_height * i32::from(action_rows) + gap_8 * i32::from(action_rows - 1);
    let client = LogicalRect::new(0, 0, client_width, actions_top + actions_height + padding);

    AddProfileLayout {
        client,
        content: LogicalRect::new(
            content.left,
            content.top,
            content.right,
            client.bottom - padding,
        ),
        name_label,
        name_edit,
        action_buttons,
        action_rows,
    }
}

fn manager_action_buttons(
    content: LogicalRect,
    top: i32,
    height: i32,
    gap: i32,
    widths: [i32; 4],
    rows: u8,
    rtl: bool,
) -> [LogicalRect; 4] {
    if rows == 1 {
        place_row(content, top, height, gap, widths, rtl)
    } else {
        let first = place_pair(content, top, height, gap, widths[0], widths[1], rtl);
        let second = place_pair(
            content,
            top + height + gap,
            height,
            gap,
            widths[2],
            widths[3],
            rtl,
        );
        [first[0], first[1], second[0], second[1]]
    }
}

fn add_action_buttons(
    content: LogicalRect,
    top: i32,
    height: i32,
    gap: i32,
    widths: [i32; 2],
    rows: u8,
    rtl: bool,
) -> [LogicalRect; 2] {
    if rows == 1 {
        place_pair(content, top, height, gap, widths[0], widths[1], rtl)
    } else if rtl {
        [
            LogicalRect::new(content.left, top, content.left + widths[0], top + height),
            LogicalRect::new(
                content.left,
                top + height + gap,
                content.left + widths[1],
                top + height * 2 + gap,
            ),
        ]
    } else {
        [
            LogicalRect::new(content.right - widths[0], top, content.right, top + height),
            LogicalRect::new(
                content.right - widths[1],
                top + height + gap,
                content.right,
                top + height * 2 + gap,
            ),
        ]
    }
}

fn place_row(
    content: LogicalRect,
    top: i32,
    height: i32,
    gap: i32,
    widths: [i32; 4],
    rtl: bool,
) -> [LogicalRect; 4] {
    if rtl {
        let first = content.left;
        let second = first + widths[0] + gap;
        let third = second + widths[1] + gap;
        let fourth = third + widths[2] + gap;
        [
            LogicalRect::new(first, top, first + widths[0], top + height),
            LogicalRect::new(second, top, second + widths[1], top + height),
            LogicalRect::new(third, top, third + widths[2], top + height),
            LogicalRect::new(fourth, top, fourth + widths[3], top + height),
        ]
    } else {
        let fourth = content.right - widths[3];
        let third = fourth - gap - widths[2];
        let second = third - gap - widths[1];
        let first = second - gap - widths[0];
        [
            LogicalRect::new(first, top, first + widths[0], top + height),
            LogicalRect::new(second, top, second + widths[1], top + height),
            LogicalRect::new(third, top, third + widths[2], top + height),
            LogicalRect::new(fourth, top, fourth + widths[3], top + height),
        ]
    }
}

fn place_pair(
    content: LogicalRect,
    top: i32,
    height: i32,
    gap: i32,
    first_width: i32,
    second_width: i32,
    rtl: bool,
) -> [LogicalRect; 2] {
    if rtl {
        let first = content.left;
        let second = first + first_width + gap;
        [
            LogicalRect::new(first, top, first + first_width, top + height),
            LogicalRect::new(second, top, second + second_width, top + height),
        ]
    } else {
        let second = content.right - second_width;
        let first = second - gap - first_width;
        [
            LogicalRect::new(first, top, first + first_width, top + height),
            LogicalRect::new(second, top, second + second_width, top + height),
        ]
    }
}
