use core::fmt;
use {Error, Result};

enum_with_unknown! {
    /// IPv6 Extension Header Option Type
    pub doc enum Type(u8) {
        /// 1 byte of padding
        Pad1 =  0,
        /// Multiple bytes of padding
        PadN =  1
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &Type::Pad1        => write!(f, "Pad1"),
            &Type::PadN        => write!(f, "PadN"),
            &Type::Unknown(id) => write!(f, "{}", id)
        }
    }
}

/// A read/write wrapper around an IPv6 Extension Header Option.
#[derive(Debug, PartialEq)]
pub struct Ipv6Option<T: AsRef<[u8]>> {
    buffer: T
}

// Format of Option
//
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+- - - - - - - - -
// |  Option Type  |  Opt Data Len |  Option Data
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+- - - - - - - - -
//
//
// See https://tools.ietf.org/html/rfc8200#section-4.2 for details.
mod field {
    #![allow(non_snake_case)]

    use wire::field::*;

    // 8-bit identifier of the type of option.
    pub const TYPE:     usize = 0;
    // 8-bit unsigned integer. Length of the DATA field of this option, in octets.
    pub const LENGTH:   usize = 1;
    // Variable-length field. Option-Type-specific data.
    pub fn DATA(length: u8) -> Field {
        2..length as usize + 2
    }
}

impl<T: AsRef<[u8]>> Ipv6Option<T> {
    /// Create a raw octet buffer with an IPv6 Extension Header Option structure.
    pub fn new(buffer: T) -> Ipv6Option<T> {
        Ipv6Option { buffer }
    }

    /// Shorthand for a combination of [new] and [check_len].
    ///
    /// [new]: #method.new
    /// [check_len]: #method.check_len
    pub fn new_checked(buffer: T) -> Result<Ipv6Option<T>> {
        let opt = Self::new(buffer);
        opt.check_len()?;
        Ok(opt)
    }

    /// Ensure that no accessor method will panic if called.
    /// Returns `Err(Error::Truncated)` if the buffer is too short.
    ///
    /// The result of this check is invalidated by calling [set_data_len].
    ///
    /// [set_data_len]: #method.set_data_len
    pub fn check_len(&self) -> Result<()> {
        let data = self.buffer.as_ref();
        let len = data.len();

        if len < field::LENGTH {
            return Err(Error::Truncated);
        }

        if self.option_type() == Type::Pad1 {
            return Ok(());
        }

        if len == field::LENGTH {
            return Err(Error::Truncated);
        }

        let df = field::DATA(data[field::LENGTH]);

        if len < df.end {
            return Err(Error::Truncated);
        }

        Ok(())
    }

    /// Consume the ipv6 option, returning the underlying buffer.
    pub fn into_inner(self) -> T {
        self.buffer
    }

    /// Return the option type.
    #[inline]
    pub fn option_type(&self) -> Type {
        let data = self.buffer.as_ref();
        Type::from(data[field::TYPE])
    }

    /// Return the length of the data.
    ///
    /// # Panics
    /// This function panics if this is an 1-byte padding option.
    #[inline]
    pub fn data_len(&self) -> u8 {
        let data = self.buffer.as_ref();
        data[field::LENGTH]
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Ipv6Option<&'a T> {
    /// Return the option data.
    ///
    /// # Panics
    /// This function panics if this is an 1-byte padding option.
    #[inline]
    pub fn data(&self) -> &'a [u8] {
        let len = self.data_len();
        let data = self.buffer.as_ref();
        &data[field::DATA(len)]
    }
}

impl<T: AsRef<[u8]> + AsMut<[u8]>> Ipv6Option<T> {
    /// Set the option type.
    #[inline]
    pub fn set_option_type(&mut self, value: Type) {
        let data = self.buffer.as_mut();
        data[field::TYPE] = value.into();
    }

    /// Set the option data length.
    ///
    /// # Panics
    /// This function panics if this is an 1-byte padding option.
    #[inline]
    pub fn set_data_len(&mut self, value: u8) {
        let data = self.buffer.as_mut();
        data[field::LENGTH] = value;
    }
}

impl<'a, T: AsRef<[u8]> + AsMut<[u8]> + ?Sized> Ipv6Option<&'a mut T> {
    /// Return a mutable pointer to the option data.
    #[inline]
    pub fn data_mut(&mut self) -> &mut [u8] {
        let len = self.data_len();
        let data = self.buffer.as_mut();
        &mut data[field::DATA(len)]
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> fmt::Display for Ipv6Option<&'a T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match Repr::parse(self) {
            Ok(repr) => write!(f, "{}", repr),
            Err(err) => {
                write!(f, "IPv6 Extension Option ({})", err)?;
                Ok(())
            }
        }
    }
}

/// A high-level representation of an IPv6 Extension Header Option.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Repr<'a> {
    Pad1,
    PadN(u8),
    Unknown {
        type_:  u8,
        length: u8,
        data:   &'a [u8]
    },

    #[doc(hidden)]
    __Nonexhaustive
}

impl<'a> Repr<'a> {
    /// Parse an IPv6 Extension Header Option and return a high-level representation.
    pub fn parse<T>(opt: &'a Ipv6Option<&'a T>) -> Result<Repr<'a>> where T: AsRef<[u8]> + ?Sized {
        match opt.option_type() {
            Type::Pad1 =>
                Ok(Repr::Pad1),
            Type::PadN =>
                Ok(Repr::PadN(opt.data_len())),
            Type::Unknown(type_) => {
                Ok(Repr::Unknown {
                    type_:  type_,
                    length: opt.data_len(),
                    data:   opt.data(),
                })
            }
        }
    }

    /// Return the length of a header that will be emitted from this high-level representation.
    pub fn buffer_len(&self) -> usize {
        match self {
            &Repr::Pad1 => 1,
            &Repr::PadN(length) =>
                field::DATA(length).end,
            &Repr::Unknown{ length, .. } =>
                field::DATA(length).end,

            &Repr::__Nonexhaustive => unreachable!()
        }
    }

    /// Emit a high-level representation into an IPv6 Extension Header Option.
    pub fn emit<T: AsRef<[u8]> + AsMut<[u8]> + ?Sized>(&self, opt: &mut Ipv6Option<&'a mut T>) {
        match self {
            &Repr::Pad1 =>
                opt.set_option_type(Type::Pad1),
            &Repr::PadN(len) => {
                opt.set_option_type(Type::PadN);
                opt.set_data_len(len);
                // Ensure all padding bytes are set to zero.
                for x in opt.data_mut().iter_mut() {
                    *x = 0
                }
            }
            &Repr::Unknown{ type_, length, data } => {
                opt.set_option_type(Type::Unknown(type_));
                opt.set_data_len(length);
                opt.data_mut().copy_from_slice(&data[..length as usize]);
            }

            &Repr::__Nonexhaustive => unreachable!()
        }
    }
}

impl<'a> fmt::Display for Repr<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "IPv6 Option ")?;
        match self {
            &Repr::Pad1 =>
                write!(f, "{} ", Type::Pad1),
            &Repr::PadN(len) =>
                write!(f, "{} length={} ", Type::PadN, len),
            &Repr::Unknown{ type_, length, .. } =>
                write!(f, "{} length={} ", Type::Unknown(type_), length),

            &Repr::__Nonexhaustive => unreachable!()
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    static IPV6OPTION_BYTES_PAD1:    [u8; 1] = [0x0];
    static IPV6OPTION_BYTES_PADN:    [u8; 3] = [0x1, 0x1, 0x0];
    static IPV6OPTION_BYTES_UNKNOWN: [u8; 5] = [0xff, 0x3, 0x0, 0x0, 0x0];

    #[test]
    fn test_check_len() {
        let bytes = [0u8];
        // zero byte buffer
        assert_eq!(Err(Error::Truncated), Ipv6Option::new(&bytes[..0]).check_len());
        // pad1
        assert_eq!(Ok(()), Ipv6Option::new(&IPV6OPTION_BYTES_PAD1).check_len());

        // padn with truncated data
        assert_eq!(Err(Error::Truncated), Ipv6Option::new(&IPV6OPTION_BYTES_PADN[..2]).check_len());
        // padn
        assert_eq!(Ok(()), Ipv6Option::new(&IPV6OPTION_BYTES_PADN).check_len());

        // unknown option type with truncated data
        assert_eq!(Err(Error::Truncated), Ipv6Option::new(&IPV6OPTION_BYTES_UNKNOWN[..4]).check_len());
        assert_eq!(Err(Error::Truncated), Ipv6Option::new(&IPV6OPTION_BYTES_UNKNOWN[..1]).check_len());
        // unknown type
        assert_eq!(Ok(()), Ipv6Option::new(&IPV6OPTION_BYTES_UNKNOWN).check_len());
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn test_data_len() {
        let opt = Ipv6Option::new(&IPV6OPTION_BYTES_PAD1);
        opt.data_len();
    }

    #[test]
    fn test_option_deconstruct() {
        // one octet of padding
        let opt = Ipv6Option::new(&IPV6OPTION_BYTES_PAD1);
        assert_eq!(opt.option_type(), Type::Pad1);

        // two octets of padding
        let bytes:  [u8; 2] = [0x1, 0x0];
        let opt = Ipv6Option::new(&bytes);
        assert_eq!(opt.option_type(), Type::PadN);
        assert_eq!(opt.data_len(), 0);

        // three octets of padding
        let opt = Ipv6Option::new(&IPV6OPTION_BYTES_PADN);
        assert_eq!(opt.option_type(), Type::PadN);
        assert_eq!(opt.data_len(), 1);
        assert_eq!(opt.data(), &[0]);

        // extra bytes in buffer
        let bytes:  [u8; 10] = [0x1, 0x7, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0xff];
        let opt = Ipv6Option::new(&bytes);
        assert_eq!(opt.option_type(), Type::PadN);
        assert_eq!(opt.data_len(), 7);
        assert_eq!(opt.data(), &[0, 0, 0, 0, 0, 0, 0]);

        // unrecognized option
        let bytes:  [u8; 1] = [0xff];
        let opt = Ipv6Option::new(&bytes);
        assert_eq!(opt.option_type(), Type::Unknown(255));

        // unrecognized option without length and data
        assert_eq!(Ipv6Option::new_checked(&bytes), Err(Error::Truncated));
    }

    #[test]
    fn test_option_parse() {
        // one octet of padding
        let opt = Ipv6Option::new(&IPV6OPTION_BYTES_PAD1);
        let pad1 = Repr::parse(&opt).unwrap();
        assert_eq!(pad1, Repr::Pad1);
        assert_eq!(pad1.buffer_len(), 1);

        // two or more octets of padding
        let opt = Ipv6Option::new(&IPV6OPTION_BYTES_PADN);
        let padn = Repr::parse(&opt).unwrap();
        assert_eq!(padn, Repr::PadN(1));
        assert_eq!(padn.buffer_len(), 3);

        // unrecognized option type
        let data = [0u8; 3];
        let opt = Ipv6Option::new(&IPV6OPTION_BYTES_UNKNOWN);
        let unknown = Repr::parse(&opt).unwrap();
        assert_eq!(unknown, Repr::Unknown { type_: 255, length: 3, data: &data});
    }

    #[test]
    fn test_option_emit() {
        let repr = Repr::Pad1;
        let mut bytes = [255u8; 1]; // don't assume bytes are initialized to zero
        let mut opt = Ipv6Option::new(&mut bytes);
        repr.emit(&mut opt);
        assert_eq!(opt.into_inner(), &IPV6OPTION_BYTES_PAD1);

        let repr = Repr::PadN(1);
        let mut bytes = [255u8; 3]; // don't assume bytes are initialized to zero
        let mut opt = Ipv6Option::new(&mut bytes);
        repr.emit(&mut opt);
        assert_eq!(opt.into_inner(), &IPV6OPTION_BYTES_PADN);

        let data = [0u8; 3];
        let repr = Repr::Unknown { type_: 255, length: 3, data: &data };
        let mut bytes = [254u8; 5]; // don't assume bytes are initialized to zero
        let mut opt = Ipv6Option::new(&mut bytes);
        repr.emit(&mut opt);
        assert_eq!(opt.into_inner(), &IPV6OPTION_BYTES_UNKNOWN);
    }
}
