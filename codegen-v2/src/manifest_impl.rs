use crate::grammar::{
    GEnumDecl, GFunctionDecl, GHeaderInclude, GMarker, GMarkers, GPrimitive, GStructDecl, GType,
    GTypeCategory,
};
use crate::manifest::*;

impl TypeInfo {
    pub fn from_g_type(ty: &GType, markers: &GMarkers) -> Result<Self> {
        pub fn get_variant(category: &GTypeCategory) -> Result<TypeVariant> {
            let variant = match category {
                GTypeCategory::Scalar(s) => match s {
                    GPrimitive::Void => TypeVariant::Void,
                    GPrimitive::Bool => TypeVariant::Bool,
                    GPrimitive::Char => TypeVariant::Int8,
                    GPrimitive::ShortInt => TypeVariant::Int16,
                    GPrimitive::Int => TypeVariant::Int32,
                    GPrimitive::UnsignedInt => TypeVariant::Uint32,
                    GPrimitive::LongInt => TypeVariant::Int64,
                    GPrimitive::Float => TypeVariant::Float32,
                    GPrimitive::Double => TypeVariant::Float64,
                    GPrimitive::UInt8T => TypeVariant::Uint8,
                    GPrimitive::UInt32T => TypeVariant::Uint32,
                },
                GTypeCategory::Struct(s) => TypeVariant::Struct(s.0 .0.to_string()),
                GTypeCategory::Enum(e) => TypeVariant::Enum(e.0 .0.to_string()),
                GTypeCategory::Pointer(_) => {
                    return Err(Error::BadType);
                }
                // TODO:
                GTypeCategory::Unrecognized(name) => TypeVariant::Struct(name.0.to_string()),
            };

            Ok(variant)
        }

        pub fn get_variant_pointer_check(cat: &GTypeCategory) -> Result<(TypeVariant, bool)> {
            let res = match cat {
                GTypeCategory::Pointer(p) => (get_variant(&*p)?, true),
                _ => (get_variant(cat)?, false),
            };

            Ok(res)
        }

        let ((variant, is_pointer), is_constant) = match ty {
            GType::Mutable(category) => (get_variant_pointer_check(category)?, false),
            GType::Const(category) => (get_variant_pointer_check(category)?, true),
            GType::Extern(_) => {
                return Err(Error::BadType);
                // TODO: Where should this be handled?
            }
        };

        Ok(TypeInfo {
            variant,
            is_constant,
            is_nullable: false,
            is_pointer,
        })
    }
}

impl ImportInfo {
    pub fn from_g_type(value: &GHeaderInclude) -> Result<Self> {
        let path: Vec<String> = value.0.split('/').map(|s| s.to_string()).collect();

        if path.is_empty() {
            return Err(Error::BadImport);
        }

        Ok(ImportInfo { path })
    }
}

impl EnumInfo {
    pub fn from_g_type(value: &GEnumDecl) -> Result<Self> {
        Ok(EnumInfo {
            name: value.name.0.to_string(),
            // Enums are always public
            // TOOD: Should be part of GEnumDecl
            is_public: true,
            variants: value
                .variants
                .iter()
                .cloned()
                .map(|(k, v)| (k.0, v))
                .collect(),
            tags: vec![],
        })
    }
}

impl StructInfo {
    pub fn from_g_type(value: &GStructDecl) -> Result<Self> {
        let mut markers = value.markers.0.iter();

        if markers.size_hint().0 != 1 {
            return Err(Error::BadObject);
        }

        // Identify whether it's a struct or a class. The object must be *one*
        // of the two available markers and is always public
        let mut tags = vec![];
        match markers.next() {
            Some(GMarker::TwExportStruct) => {
                tags.push("TW_EXPORT_STRUCT".to_string());
            }
            Some(GMarker::TwExportClass) => {
                tags.push("TW_EXPORT_CLASS".to_string());
            }
            _ => return Err(Error::BadObject),
        };

        // Process fields.
        let fields = value
            .fields
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.0.to_string(),
                    // In this context, types inside a C struct do not support
                    // markers.
                    TypeInfo::from_g_type(v, &GMarkers(vec![]))?,
                ))
            })
            .collect::<Result<Vec<(String, TypeInfo)>>>()?;

        Ok(StructInfo {
            name: value.name.0 .0.to_string(),
            is_public: true,
            fields,
            tags,
        })
    }
}

impl PropertyInfo {
    pub fn from_g_type(object: &StructInfo, value: &GFunctionDecl) -> Result<Self> {
        // ### Name

        // Strip the object name from the property name.
        // E.g. "SomeObjectIsValid" => "IsValid"
        let name = value
            .name
            .0
            .strip_prefix(&object.name)
            .ok_or(Error::BadProperty)?
            .to_string();

        if name.is_empty() {
            return Err(Error::BadProperty);
        }

        // ### Marker

        let mut markers = value.markers.0.iter();

        // Must have one marker.
        if markers.size_hint().0 != 1 {
            return Err(Error::BadProperty);
        }

        // The property must have one of the two available markers and is always public.
        let (is_static, is_public) = match markers.next() {
            Some(GMarker::TwExportProperty) => (false, true),
            Some(GMarker::TwExportStaticProperty) => (true, true),
            _ => return Err(Error::BadObject),
        };

        // ### Param

        // Must have one parameter.
        if value.params.len() != 1 {
            return Err(Error::BadProperty);
        }

        // Convert GType to TypeInfo.
        let g_ty = value.params.get(0).unwrap();
        let ty = TypeInfo::from_g_type(&g_ty.ty, &g_ty.markers)?;

        // The parameter type must be the same as the object this property
        // belongs to.
        if let TypeVariant::Struct(name) = ty.variant {
            if name != object.name {
                return Err(Error::BadProperty);
            }
        } else {
            return Err(Error::BadProperty);
        }

        // Must be a pointer and not nullable.
        if ty.is_nullable || !ty.is_pointer {
            return Err(Error::BadProperty);
        }

        // ### Return value

        // Extract return value.
        let re = &value.return_value;
        let return_type = TypeInfo::from_g_type(&re.ty, &re.markers)?;

        Ok(PropertyInfo {
            name,
            is_public,
            is_static,
            return_type,
            comments: vec![],
        })
    }
}

impl InitInfo {
    pub fn from_g_type(value: &GFunctionDecl) -> Result<Self> {
        let func = MethodInfo::from_g_type(&None, value)?;

        Ok(InitInfo {
            name: func.name,
            params: func.params,
            comments: vec![],
        })
    }
}

impl DeinitInfo {
    pub fn from_g_type(value: &GFunctionDecl) -> Result<Self> {
        let func = MethodInfo::from_g_type(&None, value)?;

        Ok(DeinitInfo {
            name: func.name,
            params: func.params,
            comments: vec![],
        })
    }
}

impl MethodInfo {
    pub fn from_g_type(object_name: &Option<String>, value: &GFunctionDecl) -> Result<Self> {
        // ### Name

        dbg!(&object_name, &value.name);

        // Strip the object name from the method name.
        // E.g. "SomeObjectIsValid" => "IsValid"
        let name = if let Some(object_name) = object_name {
            value
                .name
                .0
                .strip_prefix(object_name)
                .ok_or(Error::BadProperty)?
                .to_string()
        } else {
            value.name.0.to_string()
        };

        if name.is_empty() {
            return Err(Error::BadProperty);
        }

        // ### Marker

        let mut markers = value.markers.0.iter();

        // Must have at least one marker.
        if markers.size_hint().0 < 1 {
            return Err(Error::BadProperty);
        }

        // The method must have one of the two available markers and is always public.
        let (is_static, is_public) = match markers.next() {
            Some(GMarker::TwExportMethod) => (false, true),
            Some(GMarker::TwExportStaticMethod) => (true, true),
            _ => return Err(Error::BadObject),
        };

        // ### Params

        let mut g_params = value.params.iter();

        // Convert parameters.
        let mut params = vec![];
        while let Some(g_item) = g_params.next() {
            params.push(ParamInfo {
                name: g_item.name.0.to_string(),
                ty: TypeInfo::from_g_type(&g_item.ty, &g_item.markers)?,
            })
        }

        // ### Return value

        // Extract return value.
        let re = &value.return_value;
        let return_type = TypeInfo::from_g_type(&re.ty, &re.markers)?;

        Ok(MethodInfo {
            name,
            is_public,
            is_static,
            params,
            return_type,
            comments: vec![],
        })
    }
}