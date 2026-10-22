-- Прелюдия рантайма скриптов: синхронизация таблицы-снимка и скрытого
-- буфера канала.
--
-- Компилируется тем же компилятором, что и пользовательский скрипт, поэтому
-- циклы `buffer.readf32`/`buffer.writef32` попадают под JIT. Rust-обход
-- таблицы через ulua-rt в ~13 раз медленнее такого цикла, так что
-- копирование выполняется на стороне Luau.

-- Тело канала, каким его видит прелюдия: скрытый буфер байтов f32 LE
-- и таблица-снимок отсчётов.
type Body = {
    __buffer: buffer,
    samples: {number},
}

local function snap_channel(body: Body)
    local b: buffer = body.__buffer
    local s: {number} = body.samples
    local n = buffer.len(b) // 4
    -- Срезать хвост снимка, если кадр стал короче.
    for i = #s, n + 1, -1 do
        s[i] = nil
    end
    for i = 1, n do
        s[i] = buffer.readf32(b, (i - 1) * 4)
    end
end

local function commit_channel(body: Body)
    local b: buffer = body.__buffer
    local s: {number} = body.samples
    for i = 1, #s do
        buffer.writef32(b, (i - 1) * 4, s[i])
    end
end

return {
    snap_channel = snap_channel,
    commit_channel = commit_channel,
    refresh = snap_channel,
}
