-- Definitions API скриптов preprocess-шага: типы static_params, host, кадра
-- и канала.
--
-- Единственный источник истины для тайпчека (script::check_script) и слов
-- автодополнения редактора (script::completion_words): ручного словаря имён
-- нет, имена берутся отсюда. Синхронизацию с рантаймом держат тесты
-- defs_cover_host_functions (интроспекция host-таблицы VM) и
-- defs_cover_point_fields (сериализация StaticPointParams).
--
-- Формат — definitions-синтаксис Luau (declare), принимаемый тайпчекером
-- ulua; черновик и обоснование — docs/research/luau-lsp-editor.md (5.3, 6).
export type Channel = {
    samples: {number},
    delay: number,
}

export type Frame = {
    [number]: Channel,
}

declare static_params: {
    hv: number,
    start_time: string,
    acquisition_time: number,
    frame_len: number,
    bad_blocks: {number},
}

declare host: {
    fir: (channel: Channel, left: number, center: number, right: number) -> (),
    refresh: (channel: Channel) -> (),
}
