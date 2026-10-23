return function(frame: Frame)
    for _, channel in frame do
        host.fir(channel, 1, 0, 1)
        for i = 1, #channel.samples do
            channel.samples[i] = channel.samples[i] * 2
        end
    end
end
