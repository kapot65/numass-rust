return function(frame: Frame)
    for _, channel in frame do
        host.hpf(channel, 9)
        -- Длина канала не меняется: правки поверх фильтра видны кадру.
        channel.samples[1] = channel.samples[1] + 0.5
    end
end