use super::*;

impl Parser {
    pub(crate) fn parse_shell_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance();

        self.expect_with_context(TokenType::Assign, "after `shell`")?;

        let value = self.parse_simple_backtick()?;
        self.expect_with_context(TokenType::Semicolon, "after shell declaration")?;

        Ok(Stmt::ShellDecl { value })
    }

    pub(crate) fn parse_sanctuary_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance();

        self.expect_with_context(TokenType::Assign, "after `sanctuary`")?;

        let value = self.parse_expr()?;
        self.expect_with_context(TokenType::Semicolon, "after sanctuary declaration")?;

        Ok(Stmt::SanctuaryDecl { value })
    }

    pub(crate) fn parse_import_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance();

        let path = self.parse_expr()?;
        self.expect_with_context(TokenType::Semicolon, "after import path")?;

        Ok(Stmt::ImportDecl { path })
    }

    pub(crate) fn parse_var_decl(&mut self) -> Result<Stmt, ParseError> {
        let (var_type, name, value) = self.parse_var_decl_common()?;
        Ok(Stmt::VarDecl {
            var_type,
            name,
            value,
        })
    }
}
