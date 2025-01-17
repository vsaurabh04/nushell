use crate::errors::ShellError;
#[cfg(not(coloring_in_tokens))]
use crate::parser::hir::syntax_shape::FlatShape;
use crate::parser::{
    hir,
    hir::syntax_shape::{
        color_fallible_syntax, color_syntax_with, continue_expression, expand_expr, expand_syntax,
        DelimitedShape, ExpandContext, ExpandExpression, ExpressionContinuationShape,
        ExpressionListShape, FallibleColorSyntax, MemberShape, PathTailShape, VariablePathShape,
    },
    hir::tokens_iterator::TokensIterator,
    parse::token_tree::Delimiter,
    RawToken, TokenNode,
};
use crate::{Span, Spanned, SpannedItem};

#[derive(Debug, Copy, Clone)]
pub struct AnyBlockShape;

#[cfg(not(coloring_in_tokens))]
impl FallibleColorSyntax for AnyBlockShape {
    type Info = ();
    type Input = ();

    fn color_syntax<'a, 'b>(
        &self,
        _input: &(),
        token_nodes: &'b mut TokensIterator<'a>,
        context: &ExpandContext,
        shapes: &mut Vec<Spanned<FlatShape>>,
    ) -> Result<(), ShellError> {
        let block = token_nodes.peek_non_ws().not_eof("block");

        let block = match block {
            Err(_) => return Ok(()),
            Ok(block) => block,
        };

        // is it just a block?
        let block = block.node.as_block();

        match block {
            // If so, color it as a block
            Some((children, spans)) => {
                let mut token_nodes = TokensIterator::new(children.item, context.span, false);
                color_syntax_with(
                    &DelimitedShape,
                    &(Delimiter::Brace, spans.0, spans.1),
                    &mut token_nodes,
                    context,
                    shapes,
                );

                return Ok(());
            }
            _ => {}
        }

        // Otherwise, look for a shorthand block. If none found, fail
        color_fallible_syntax(&ShorthandBlock, token_nodes, context, shapes)
    }
}

#[cfg(coloring_in_tokens)]
impl FallibleColorSyntax for AnyBlockShape {
    type Info = ();
    type Input = ();

    fn name(&self) -> &'static str {
        "AnyBlockShape"
    }

    fn color_syntax<'a, 'b>(
        &self,
        _input: &(),
        token_nodes: &'b mut TokensIterator<'a>,
        context: &ExpandContext,
    ) -> Result<(), ShellError> {
        let block = token_nodes.peek_non_ws().not_eof("block");

        let block = match block {
            Err(_) => return Ok(()),
            Ok(block) => block,
        };

        // is it just a block?
        let block = block.node.as_block();

        match block {
            // If so, color it as a block
            Some((children, spans)) => {
                token_nodes.child(children, |token_nodes| {
                    color_syntax_with(
                        &DelimitedShape,
                        &(Delimiter::Brace, spans.0, spans.1),
                        token_nodes,
                        context,
                    );
                });

                return Ok(());
            }
            _ => {}
        }

        // Otherwise, look for a shorthand block. If none found, fail
        color_fallible_syntax(&ShorthandBlock, token_nodes, context)
    }
}

impl ExpandExpression for AnyBlockShape {
    fn expand_expr<'a, 'b>(
        &self,
        token_nodes: &mut TokensIterator<'_>,
        context: &ExpandContext,
    ) -> Result<hir::Expression, ShellError> {
        let block = token_nodes.peek_non_ws().not_eof("block")?;

        // is it just a block?
        let block = block.node.as_block();

        match block {
            Some((block, _tags)) => {
                let mut iterator = TokensIterator::new(&block.item, context.span, false);

                let exprs = expand_syntax(&ExpressionListShape, &mut iterator, context)?;

                return Ok(hir::RawExpression::Block(exprs).spanned(block.span));
            }
            _ => {}
        }

        expand_syntax(&ShorthandBlock, token_nodes, context)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ShorthandBlock;

#[cfg(not(coloring_in_tokens))]
impl FallibleColorSyntax for ShorthandBlock {
    type Info = ();
    type Input = ();

    fn color_syntax<'a, 'b>(
        &self,
        _input: &(),
        token_nodes: &'b mut TokensIterator<'a>,
        context: &ExpandContext,
        shapes: &mut Vec<Spanned<FlatShape>>,
    ) -> Result<(), ShellError> {
        // Try to find a shorthand head. If none found, fail
        color_fallible_syntax(&ShorthandPath, token_nodes, context, shapes)?;

        loop {
            // Check to see whether there's any continuation after the head expression
            let result =
                color_fallible_syntax(&ExpressionContinuationShape, token_nodes, context, shapes);

            match result {
                // if no continuation was found, we're done
                Err(_) => break,
                // if a continuation was found, look for another one
                Ok(_) => continue,
            }
        }

        Ok(())
    }
}

#[cfg(coloring_in_tokens)]
impl FallibleColorSyntax for ShorthandBlock {
    type Info = ();
    type Input = ();

    fn name(&self) -> &'static str {
        "ShorthandBlock"
    }

    fn color_syntax<'a, 'b>(
        &self,
        _input: &(),
        token_nodes: &'b mut TokensIterator<'a>,
        context: &ExpandContext,
    ) -> Result<(), ShellError> {
        // Try to find a shorthand head. If none found, fail
        color_fallible_syntax(&ShorthandPath, token_nodes, context)?;

        loop {
            // Check to see whether there's any continuation after the head expression
            let result = color_fallible_syntax(&ExpressionContinuationShape, token_nodes, context);

            match result {
                // if no continuation was found, we're done
                Err(_) => break,
                // if a continuation was found, look for another one
                Ok(_) => continue,
            }
        }

        Ok(())
    }
}

impl ExpandExpression for ShorthandBlock {
    fn expand_expr<'a, 'b>(
        &self,
        token_nodes: &'b mut TokensIterator<'a>,
        context: &ExpandContext,
    ) -> Result<hir::Expression, ShellError> {
        let path = expand_expr(&ShorthandPath, token_nodes, context)?;
        let start = path.span;
        let expr = continue_expression(path, token_nodes, context)?;
        let end = expr.span;
        let block = hir::RawExpression::Block(vec![expr]).spanned(start.until(end));

        Ok(block)
    }
}

/// A shorthand for `$it.foo."bar"`, used inside of a shorthand block
#[derive(Debug, Copy, Clone)]
pub struct ShorthandPath;

#[cfg(not(coloring_in_tokens))]
impl FallibleColorSyntax for ShorthandPath {
    type Info = ();
    type Input = ();

    fn color_syntax<'a, 'b>(
        &self,
        _input: &(),
        token_nodes: &'b mut TokensIterator<'a>,
        context: &ExpandContext,
        shapes: &mut Vec<Spanned<FlatShape>>,
    ) -> Result<(), ShellError> {
        token_nodes.atomic(|token_nodes| {
            let variable = color_fallible_syntax(&VariablePathShape, token_nodes, context, shapes);

            match variable {
                Ok(_) => {
                    // if it's a variable path, that's the head part
                    return Ok(());
                }

                Err(_) => {
                    // otherwise, we'll try to find a member path
                }
            }

            // look for a member (`<member>` -> `$it.<member>`)
            color_fallible_syntax(&MemberShape, token_nodes, context, shapes)?;

            // Now that we've synthesized the head, of the path, proceed to expand the tail of the path
            // like any other path.
            let tail = color_fallible_syntax(&PathTailShape, token_nodes, context, shapes);

            match tail {
                Ok(_) => {}
                Err(_) => {
                    // It's ok if there's no path tail; a single member is sufficient
                }
            }

            Ok(())
        })
    }
}

#[cfg(coloring_in_tokens)]
impl FallibleColorSyntax for ShorthandPath {
    type Info = ();
    type Input = ();

    fn name(&self) -> &'static str {
        "ShorthandPath"
    }

    fn color_syntax<'a, 'b>(
        &self,
        _input: &(),
        token_nodes: &'b mut TokensIterator<'a>,
        context: &ExpandContext,
    ) -> Result<(), ShellError> {
        token_nodes.atomic(|token_nodes| {
            let variable = color_fallible_syntax(&VariablePathShape, token_nodes, context);

            match variable {
                Ok(_) => {
                    // if it's a variable path, that's the head part
                    return Ok(());
                }

                Err(_) => {
                    // otherwise, we'll try to find a member path
                }
            }

            // look for a member (`<member>` -> `$it.<member>`)
            color_fallible_syntax(&MemberShape, token_nodes, context)?;

            // Now that we've synthesized the head, of the path, proceed to expand the tail of the path
            // like any other path.
            let tail = color_fallible_syntax(&PathTailShape, token_nodes, context);

            match tail {
                Ok(_) => {}
                Err(_) => {
                    // It's ok if there's no path tail; a single member is sufficient
                }
            }

            Ok(())
        })
    }
}

impl ExpandExpression for ShorthandPath {
    fn expand_expr<'a, 'b>(
        &self,
        token_nodes: &'b mut TokensIterator<'a>,
        context: &ExpandContext,
    ) -> Result<hir::Expression, ShellError> {
        // if it's a variable path, that's the head part
        let path = expand_expr(&VariablePathShape, token_nodes, context);

        match path {
            Ok(path) => return Ok(path),
            Err(_) => {}
        }

        // Synthesize the head of the shorthand path (`<member>` -> `$it.<member>`)
        let mut head = expand_expr(&ShorthandHeadShape, token_nodes, context)?;

        // Now that we've synthesized the head, of the path, proceed to expand the tail of the path
        // like any other path.
        let tail = expand_syntax(&PathTailShape, token_nodes, context);

        match tail {
            Err(_) => return Ok(head),
            Ok((tail, _)) => {
                // For each member that `PathTailShape` expanded, join it onto the existing expression
                // to form a new path
                for member in tail {
                    head = hir::Expression::dot_member(head, member);
                }

                Ok(head)
            }
        }
    }
}

/// A shorthand for `$it.foo."bar"`, used inside of a shorthand block
#[derive(Debug, Copy, Clone)]
pub struct ShorthandHeadShape;

#[cfg(not(coloring_in_tokens))]
impl FallibleColorSyntax for ShorthandHeadShape {
    type Info = ();
    type Input = ();

    fn color_syntax<'a, 'b>(
        &self,
        _input: &(),
        token_nodes: &'b mut TokensIterator<'a>,
        _context: &ExpandContext,
        shapes: &mut Vec<Spanned<FlatShape>>,
    ) -> Result<(), ShellError> {
        // A shorthand path must not be at EOF
        let peeked = token_nodes.peek_non_ws().not_eof("shorthand path")?;

        match peeked.node {
            // If the head of a shorthand path is a bare token, it expands to `$it.bare`
            TokenNode::Token(Spanned {
                item: RawToken::Bare,
                span,
            }) => {
                peeked.commit();
                shapes.push(FlatShape::BareMember.spanned(*span));
                Ok(())
            }

            // If the head of a shorthand path is a string, it expands to `$it."some string"`
            TokenNode::Token(Spanned {
                item: RawToken::String(_),
                span: outer,
            }) => {
                peeked.commit();
                shapes.push(FlatShape::StringMember.spanned(*outer));
                Ok(())
            }

            other => Err(ShellError::type_error(
                "shorthand head",
                other.tagged_type_name(),
            )),
        }
    }
}

#[cfg(coloring_in_tokens)]
#[cfg(not(coloring_in_tokens))]
impl FallibleColorSyntax for ShorthandHeadShape {
    type Info = ();
    type Input = ();

    fn color_syntax<'a, 'b>(
        &self,
        _input: &(),
        token_nodes: &'b mut TokensIterator<'a>,
        _context: &ExpandContext,
        shapes: &mut Vec<Spanned<FlatShape>>,
    ) -> Result<(), ShellError> {
        // A shorthand path must not be at EOF
        let peeked = token_nodes.peek_non_ws().not_eof("shorthand path")?;

        match peeked.node {
            // If the head of a shorthand path is a bare token, it expands to `$it.bare`
            TokenNode::Token(Spanned {
                item: RawToken::Bare,
                span,
            }) => {
                peeked.commit();
                shapes.push(FlatShape::BareMember.spanned(*span));
                Ok(())
            }

            // If the head of a shorthand path is a string, it expands to `$it."some string"`
            TokenNode::Token(Spanned {
                item: RawToken::String(_),
                span: outer,
            }) => {
                peeked.commit();
                shapes.push(FlatShape::StringMember.spanned(*outer));
                Ok(())
            }

            other => Err(ShellError::type_error(
                "shorthand head",
                other.tagged_type_name(),
            )),
        }
    }
}

impl ExpandExpression for ShorthandHeadShape {
    fn expand_expr<'a, 'b>(
        &self,
        token_nodes: &'b mut TokensIterator<'a>,
        context: &ExpandContext,
    ) -> Result<hir::Expression, ShellError> {
        // A shorthand path must not be at EOF
        let peeked = token_nodes.peek_non_ws().not_eof("shorthand path")?;

        match peeked.node {
            // If the head of a shorthand path is a bare token, it expands to `$it.bare`
            TokenNode::Token(Spanned {
                item: RawToken::Bare,
                span,
            }) => {
                // Commit the peeked token
                peeked.commit();

                // Synthesize an `$it` expression
                let it = synthetic_it();

                // Make a path out of `$it` and the bare token as a member
                Ok(hir::Expression::path(
                    it,
                    vec![span.spanned_string(context.source)],
                    *span,
                ))
            }

            // If the head of a shorthand path is a string, it expands to `$it."some string"`
            TokenNode::Token(Spanned {
                item: RawToken::String(inner),
                span: outer,
            }) => {
                // Commit the peeked token
                peeked.commit();

                // Synthesize an `$it` expression
                let it = synthetic_it();

                // Make a path out of `$it` and the bare token as a member
                Ok(hir::Expression::path(
                    it,
                    vec![inner.string(context.source).spanned(*outer)],
                    *outer,
                ))
            }

            // Any other token is not a valid bare head
            other => {
                return Err(ShellError::type_error(
                    "shorthand path",
                    other.tagged_type_name(),
                ))
            }
        }
    }
}

fn synthetic_it() -> hir::Expression {
    hir::Expression::it_variable(Span::unknown(), Span::unknown())
}
