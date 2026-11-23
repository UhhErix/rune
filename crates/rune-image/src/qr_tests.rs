use super::*;

#[test]
fn qr_code_finder_timing_dark_module_and_format_bits_are_valid() {
    let qr = QrCode::encode("https://s.team/q/1/1234567890").expect("encode Steam URL");
    assert_eq!(qr.ec_level(), EcLevel::Medium);
    assert!(qr.version() >= 2 && qr.version() <= 4);
    let s = qr.size();
    assert_eq!(s, 17 + 4 * usize::from(qr.version()));

    // Verify all three 7x7 finder patterns (outer dark ring, inner light ring, 3x3 dark core).
    for &(ox, oy) in &[(0usize, 0usize), (s - 7, 0), (0, s - 7)] {
        for dy in 0..7 {
            for dx in 0..7 {
                let edge = dx == 0 || dx == 6 || dy == 0 || dy == 6;
                let core = (2..=4).contains(&dx) && (2..=4).contains(&dy);
                assert_eq!(
                    qr.is_dark(ox + dx, oy + dy),
                    edge || core,
                    "finder at ({ox},{oy})+({dx},{dy})"
                );
            }
        }
    }
    // Fixed dark module at (8, s - 8).
    assert!(qr.is_dark(8, s - 8));
    // Timing pattern alternating along row 6 and col 6.
    for i in 8..s - 8 {
        assert_eq!(qr.is_dark(i, 6), i % 2 == 0);
        assert_eq!(qr.is_dark(6, i), i % 2 == 0);
    }
    // Format bits match both copies and pass BCH(15, 5) XOR 0x5412.
    let mut copy1 = 0u32;
    for i in 0..=5 {
        copy1 |= u32::from(qr.is_dark(8, i)) << i;
    }
    copy1 |= u32::from(qr.is_dark(8, 7)) << 6;
    copy1 |= u32::from(qr.is_dark(8, 8)) << 7;
    copy1 |= u32::from(qr.is_dark(7, 8)) << 8;
    for i in 9..15 {
        copy1 |= u32::from(qr.is_dark(14 - i, 8)) << i;
    }
    let mut copy2 = 0u32;
    for i in 0..8 {
        copy2 |= u32::from(qr.is_dark(s - 1 - i, 8)) << i;
    }
    for i in 8..15 {
        copy2 |= u32::from(qr.is_dark(8, s - 15 + i)) << i;
    }
    assert_eq!(copy1, copy2, "format info copies must match");
    let unmasked = copy1 ^ 0x5412;
    let ec_bits = (unmasked >> 13) & 0b11;
    let mask_bits = (unmasked >> 10) & 0b111;
    assert_eq!(ec_bits, EcLevel::Medium.format_bits());
    assert_eq!(mask_bits, u32::from(qr.mask()));
}

#[test]
fn qr_reed_solomon_codewords_have_zero_syndrome_across_versions() {
    for &(version, ec, len) in &[
        (1u8, EcLevel::Low, 10usize),
        (1, EcLevel::Medium, 14),
        (3, EcLevel::Quartile, 30),
        (5, EcLevel::High, 40),
        (7, EcLevel::Medium, 110),
        (10, EcLevel::Medium, 200),
    ] {
        let payload: Vec<u8> = (0..len).map(|i| b'A' + (i % 26) as u8).collect();
        let data_cap = data_capacity_bytes(version, ec);
        let cw = build_codewords(&payload, version, ec, data_cap);
        assert_eq!(cw.len(), TOTAL_CODEWORDS[usize::from(version - 1)]);

        // De-interleave blocks and verify every block evaluates to 0 at every root alpha^0..alpha^(ec_len-1).
        let (ec_len, num_blocks) = EC_TABLE[usize::from(version - 1)][ec.index()];
        let total = cw.len();
        let short_blocks = num_blocks - (total % num_blocks);
        let short_data_len = total / num_blocks - ec_len;
        let max_data = short_data_len + usize::from(short_blocks < num_blocks);

        let mut blocks: Vec<Vec<u8>> = (0..num_blocks).map(|_| Vec::new()).collect();
        let mut cursor = 0;
        for i in 0..max_data {
            for (b, block) in blocks.iter_mut().enumerate() {
                let len = short_data_len + usize::from(b >= short_blocks);
                if i < len {
                    block.push(cw[cursor]);
                    cursor += 1;
                }
            }
        }
        for _ in 0..ec_len {
            for block in &mut blocks {
                block.push(cw[cursor]);
                cursor += 1;
            }
        }
        assert_eq!(cursor, total);

        for block in &blocks {
            let mut alpha = 1u8;
            for _ in 0..ec_len {
                let mut eval = 0u8;
                for &coeff in block {
                    eval = gf_mul(eval, alpha) ^ coeff;
                }
                assert_eq!(
                    eval, 0,
                    "RS syndrome must be zero for version {version} {ec:?}"
                );
                alpha = gf_mul(alpha, 0x02);
            }
        }
    }
}

#[test]
fn qr_version_7_to_10_and_capacity_bounds() {
    let long_url = format!("https://s.team/q/1/{}", "x".repeat(160));
    let qr = QrCode::encode(&long_url).expect("encode 179-byte payload");
    assert!(qr.version() >= 9);

    let too_long = vec![b'a'; 300];
    assert!(matches!(
        QrCode::encode(&too_long),
        Err(QrError::DataTooLong { .. })
    ));
}

#[test]
fn qr_to_png_round_trips_through_pixels_from_png() {
    let qr = QrCode::encode("https://s.team/q/1/abc").expect("encode");
    let png = qr.to_png(4, 4).expect("encode png");
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));

    let px = Pixels::from_png(&png).expect("decode png into Pixels");
    let expected_side = (qr.size() as u32 + 8) * 4;
    assert_eq!(px.size(), (expected_side, expected_side));

    let (w, h, rgba) = px.to_rgba8();
    assert_eq!((w, h), (expected_side, expected_side));
    // Top-left corner is quiet zone (white).
    assert_eq!(&rgba[0..4], &[255, 255, 255, 255]);
    // Top-left finder module (4, 4) in modules -> pixel (16, 16) must be black.
    let idx = ((16 * w + 16) * 4) as usize;
    assert_eq!(&rgba[idx..idx + 4], &[0, 0, 0, 255]);
}
