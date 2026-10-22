-- Аналог PreprocessParams::average: вычитает среднее первых HEAD_BINS
-- отсчётов и инвертирует сигнал. Арифметика выполняется в Luau, без
-- отдельной обвязки на каждую операцию.
--
-- Общая структура скрипта (сигнатура, формат кадра, глобалы, статические
-- локалы) — в подсказке редактора скриптов (widgets.rs) и в defs.d.luau.

-- HEAD_BINS совпадает с константой Rust. Если отсчётов в кадре меньше
-- HEAD_BINS, среднее считается по всем отсчётам.
local HEAD_BINS: number = 30

return function(frame: Frame)
    for _, channel in frame do
        local samples: {number} = channel.samples
        local count: number = math.min(HEAD_BINS, #samples)
        local sum: number = 0
        for index = 1, count do
            sum += samples[index]
        end
        local baseline: number = if count > 0 then sum / count else 0

        -- Вычесть голову и инвертировать: -(sample - baseline).
        for index = 1, #samples do
            samples[index] = baseline - samples[index]
        end
    end
end
